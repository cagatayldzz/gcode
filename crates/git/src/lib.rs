//! Git access for Vekro. Never depends on the interface.

use git2::{Repository, StatusOptions};
use std::path::Path;

/// What the sidebar shows about a project's repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
  /// The checked-out branch, or a short commit id when HEAD is detached.
  pub branch: String,
  /// Files with changes in the working tree or the index, including untracked ones.
  pub changed: usize,
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
  let changed =
    repository.statuses(Some(&mut options)).map(|statuses| statuses.iter().filter(|entry| !entry.status().is_empty()).count()).unwrap_or(0);
  Some(Summary { branch, changed })
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
