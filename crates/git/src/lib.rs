//! Git access for Vekro. Never depends on the interface.

use git2::{DiffFormat, DiffOptions, Repository, Status, StatusOptions};
use std::path::{Path, PathBuf};

/// How a file differs from the last commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
  New,
  Modified,
  Deleted,
  Renamed,
  Conflicted,
}

impl ChangeKind {
  /// The one-letter mark shown beside the file.
  pub fn mark(self) -> &'static str {
    match self {
      Self::New => "A",
      Self::Modified => "M",
      Self::Deleted => "D",
      Self::Renamed => "R",
      Self::Conflicted => "!",
    }
  }
}

/// A changed file, with its path relative to the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
  pub path: PathBuf,
  pub kind: ChangeKind,
}

/// What the sidebar shows about a project's repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
  /// The checked-out branch, or a short commit id when HEAD is detached.
  pub branch: String,
  /// Files with changes in the working tree or the index, including untracked ones, sorted by path.
  pub changes: Vec<Change>,
}

impl Summary {
  pub fn changed(&self) -> usize {
    self.changes.len()
  }
}

/// Reads the repository containing `path`; `None` when it is not inside one.
pub fn summary(path: &Path) -> Option<Summary> {
  let repository = Repository::discover(path).ok()?;
  let branch = match repository.head() {
    Ok(head) if head.is_branch() => head.shorthand().unwrap_or("HEAD").to_string(),
    Ok(head) => head.target().map(|id| id.to_string().chars().take(7).collect()).unwrap_or_else(|| "HEAD".into()),
    // A repository without commits still names its branch.
    Err(_) => {
      repository.find_reference("HEAD").ok().and_then(|head| head.symbolic_target().map(|name| name.trim_start_matches("refs/heads/").to_string()))?
    }
  };
  let mut options = StatusOptions::new();
  options.include_untracked(true).recurse_untracked_dirs(true);
  let mut changes: Vec<Change> = repository
    .statuses(Some(&mut options))
    .map(|statuses| {
      statuses
        .iter()
        .filter_map(|entry| {
          let status = entry.status();
          let kind = if status.contains(Status::CONFLICTED) {
            ChangeKind::Conflicted
          } else if status.intersects(Status::INDEX_NEW | Status::WT_NEW) {
            ChangeKind::New
          } else if status.intersects(Status::INDEX_DELETED | Status::WT_DELETED) {
            ChangeKind::Deleted
          } else if status.intersects(Status::INDEX_RENAMED | Status::WT_RENAMED) {
            ChangeKind::Renamed
          } else if status.is_empty() || status == Status::IGNORED {
            return None;
          } else {
            ChangeKind::Modified
          };
          Some(Change { path: PathBuf::from(entry.path()?), kind })
        })
        .collect()
    })
    .unwrap_or_default();
  changes.sort_by(|a, b| a.path.cmp(&b.path));
  Some(Summary { branch, changes })
}

/// The folder name `git clone` would pick for `url`: `https://github.com/a/vekro.git` gives `vekro`.
pub fn repository_name(url: &str) -> Option<String> {
  let trimmed = url.trim().trim_end_matches('/');
  let last = trimmed.rsplit(['/', ':']).next()?;
  let name = last.strip_suffix(".git").unwrap_or(last);
  (!name.is_empty()).then(|| name.to_string())
}

/// Creates the folder `parent/name` (it must not exist yet) and starts a repository on `main` in it.
pub fn create_project(parent: &Path, name: &str) -> Result<std::path::PathBuf, String> {
  let name = name.trim();
  if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
    return Err("Enter a project name without slashes.".into());
  }
  let path = parent.join(name);
  if path.exists() {
    return Err(format!("{} already exists.", path.display()));
  }
  std::fs::create_dir_all(&path).map_err(|error| format!("Could not create {}: {error}", path.display()))?;
  Repository::init_opts(&path, git2::RepositoryInitOptions::new().initial_head("main")).map_err(|error| error.message().to_string())?;
  Ok(path)
}

/// Clones `url` into `parent/<repository name>` with the `git` program, so the user's own SSH keys and
/// credential helpers apply. It never asks for input: a repository that needs a login fails with git's message.
pub fn clone(url: &str, parent: &Path) -> Result<std::path::PathBuf, String> {
  let url = url.trim();
  let name = repository_name(url).ok_or("Enter a repository URL.")?;
  let path = parent.join(&name);
  if path.exists() {
    return Err(format!("{} already exists.", path.display()));
  }
  let output = std::process::Command::new("git")
    .arg("clone")
    .arg("--")
    .arg(url)
    .arg(&path)
    .env("GIT_TERMINAL_PROMPT", "0")
    .output()
    .map_err(|error| format!("Could not run git: {error}"))?;
  if output.status.success() {
    Ok(path)
  } else {
    let message = String::from_utf8_lossy(&output.stderr);
    Err(message.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("git clone failed.").trim().to_string())
  }
}

/// The unified diff of one file against the last commit (staged and unstaged changes together, untracked
/// files as new). `file` is relative to the repository.
pub fn file_diff(path: &Path, file: &Path) -> Result<String, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let head = repository.head().ok().and_then(|head| head.peel_to_tree().ok());
  let mut options = DiffOptions::new();
  options.pathspec(file).include_untracked(true).show_untracked_content(true).recurse_untracked_dirs(true);
  let diff = repository.diff_tree_to_workdir_with_index(head.as_ref(), Some(&mut options)).map_err(|error| error.message().to_string())?;
  let mut patch = String::new();
  diff
    .print(DiffFormat::Patch, |_, _, line| {
      if matches!(line.origin(), '+' | '-' | ' ') {
        patch.push(line.origin());
      }
      patch.push_str(&String::from_utf8_lossy(line.content()));
      true
    })
    .map_err(|error| error.message().to_string())?;
  Ok(patch)
}
