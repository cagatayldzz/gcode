//! Git access for the app. Never depends on the interface.

use git2::{build::CheckoutBuilder, DiffFormat, DiffOptions, Repository, Status, StatusOptions};
use std::path::{Path, PathBuf};

pub mod conflict_markers;
pub mod editor_diff;

mod branch;
pub use branch::{
  branch_entries, create_branch_with, delete_branch, delete_remote_branch, discard_stash, discard_stash_by_id, has_changes, has_stash, rename_branch,
  restore_stash, restore_stash_by_id, stash_changes, stash_ids, switch_branch_with, upstream_remote, BranchEntry, Changes, SWITCH_OVERWRITE,
};

mod commit_actions;
pub use commit_actions::{add_to_gitignore, branch_from_commit, cherry_pick, revert_commit};

mod compare;

mod merge;
mod operation;

pub use operation::{operation, resume_operation, Operation, OperationStep};

pub mod shell_path;
pub use merge::{read_merge, start_merge, MergeOutcome, MergeSide, MergeState};

pub use compare::{compare, compare_choices, merge_branch, BranchChoices, BranchRef, Comparison, COMPARE_LIMIT};

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
  /// Where a renamed file was before, relative to the repository.
  pub from: Option<PathBuf>,
}

/// What the sidebar shows about a project's repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
  /// The checked-out branch, or a short commit id when HEAD is detached.
  pub branch: String,
  /// Files with changes in the working tree or the index, including untracked ones, sorted by path.
  pub changes: Vec<Change>,
  /// Where the branch stands against its remote; `None` when the repository has no remote.
  pub sync: Option<Sync>,
  pub operation: Operation,
}

/// How the checked-out branch relates to its remote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sync {
  /// The remote the branch follows, or the first remote while the branch has no upstream yet (`origin` when there is one).
  pub remote: String,
  /// The remote's address.
  pub url: String,
  /// The branch has an upstream branch on the remote.
  pub upstream: bool,
  /// Commits only here, to push.
  pub ahead: usize,
  /// Commits only on the remote, to pull (as of the last fetch).
  pub behind: usize,
  /// When the remote was last fetched, from the time of `FETCH_HEAD`; `None` if never.
  pub last_fetched: Option<std::time::SystemTime>,
}

fn sync_status(repository: &Repository) -> Option<Sync> {
  let remotes = repository.remotes().ok()?;
  let head = repository.head().ok().filter(|head| head.is_branch());
  let name = head.as_ref().and_then(|head| head.shorthand().ok()).map(str::to_string);
  let upstream =
    name.as_deref().and_then(|name| repository.find_branch(name, git2::BranchType::Local).ok()).and_then(|branch| branch.upstream().ok());
  let followed = head
    .as_ref()
    .and_then(|head| head.name().ok())
    .and_then(|name| repository.branch_upstream_remote(name).ok())
    .and_then(|remote| remote.as_str().ok().map(str::to_string));
  let remote = match followed {
    Some(remote) => remote,
    None => remotes.iter().flatten().flatten().find(|remote| *remote == "origin").or_else(|| remotes.iter().flatten().flatten().next())?.to_string(),
  };
  let (ahead, behind) = match (head.as_ref().and_then(|head| head.target()), upstream.as_ref().and_then(|upstream| upstream.get().target())) {
    (Some(local), Some(remote)) => repository.graph_ahead_behind(local, remote).unwrap_or((0, 0)),
    _ => (0, 0),
  };
  let last_fetched = std::fs::metadata(repository.path().join("FETCH_HEAD")).and_then(|meta| meta.modified()).ok();
  let url = repository.find_remote(&remote).ok().and_then(|found| found.url().ok().map(str::to_string)).unwrap_or_default();
  Some(Sync { remote, url, upstream: upstream.is_some(), ahead, behind, last_fetched })
}

impl Summary {
  pub fn changed(&self) -> usize {
    self.changes.len()
  }

  /// How many files have changes, and whether commits are waiting to be pushed (the branch is ahead of its remote, or was
  /// never published while the repository has a remote).
  pub fn status(&self) -> (usize, bool) {
    let push = self.sync.as_ref().is_some_and(|sync| sync.ahead > 0 || !sync.upstream);
    (self.changed(), push)
  }
}

/// Reads the repository containing `path`; `None` when it is not inside one.
/// The ignore rules (`.gitignore`, `.git/info/exclude`, global excludes) of the repository around a folder.
pub struct IgnoreRules {
  repository: Repository,
  root: PathBuf,
  /// Where `root` lies inside the working tree.
  base: PathBuf,
}

impl IgnoreRules {
  /// `None` outside a repository.
  pub fn open(root: &Path) -> Option<Self> {
    let repository = Repository::discover(root).ok()?;
    let base = root.canonicalize().ok()?.strip_prefix(repository.workdir()?.canonicalize().ok()?).ok()?.to_path_buf();
    Some(Self { repository, root: root.to_path_buf(), base })
  }

  /// Whether Git ignores `path`, which lies inside the folder given to `open`.
  pub fn is_ignored(&self, path: &Path) -> bool {
    path.strip_prefix(&self.root).is_ok_and(|relative| self.repository.is_path_ignored(self.base.join(relative)).unwrap_or(false))
  }
}

pub fn summary(path: &Path) -> Option<Summary> {
  let repository = Repository::discover(path).ok()?;
  let branch = match repository.head() {
    Ok(head) if head.is_branch() => head.shorthand().unwrap_or("HEAD").to_string(),
    Ok(head) => head.target().map(|id| id.to_string().chars().take(7).collect()).unwrap_or_else(|| "HEAD".into()),
    // A repository without commits still names its branch.
    Err(_) => repository
      .find_reference("HEAD")
      .ok()
      .and_then(|head| head.symbolic_target().ok().flatten().map(|name| name.trim_start_matches("refs/heads/").to_string()))?,
  };
  let mut options = StatusOptions::new();
  options.include_untracked(true).recurse_untracked_dirs(true).include_ignored(false).renames_head_to_index(true).renames_index_to_workdir(true);
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
          // A renamed file's `entry.path()` is its old path: the new one is in the delta.
          let rename = (kind == ChangeKind::Renamed).then(|| entry.head_to_index().or_else(|| entry.index_to_workdir())).flatten();
          let path =
            rename.as_ref().and_then(|delta| delta.new_file().path()).map(Path::to_path_buf).or_else(|| entry.path().ok().map(PathBuf::from))?;
          let from = rename.as_ref().and_then(|delta| delta.old_file().path()).map(Path::to_path_buf);
          Some(Change { path, kind, from })
        })
        .collect()
    })
    .unwrap_or_default();
  changes.sort_by(|a, b| a.path.cmp(&b.path));
  let sync = sync_status(&repository);
  Some(Summary { branch, changes, sync, operation: Operation::read(&repository) })
}

/// What the project menu shows beside a project: how many files have changes, and whether commits are waiting to be pushed
/// (the branch is ahead of its remote, or was never published while the repository has a remote). `None` outside a repository.
pub fn quick_status(path: &Path) -> Option<(usize, bool)> {
  Some(summary(path)?.status())
}

/// The folder name `git clone` would pick for `url`: `https://github.com/a/project.git` gives `project`.
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
pub fn clone(url: &str, parent: &Path, credentials: Option<&Credentials>) -> Result<std::path::PathBuf, String> {
  let url = url.trim();
  let name = repository_name(url).ok_or("Enter a repository URL.")?;
  let path = parent.join(&name);
  if path.exists() {
    return Err(format!("{} already exists.", path.display()));
  }
  let mut command = std::process::Command::new("git");
  shell_path::apply(&mut command);
  credential_options(&mut command, credentials);
  let output = command
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
/// files as new), with the whole file as context. `file` is relative to the repository.
pub fn file_diff(path: &Path, file: &Path) -> Result<String, String> {
  working_tree_patch(path, Some(file), None)
}

/// [`file_diff`] of a renamed file: `from` is where it was before, so the patch shows what changed inside it instead of a deleted
/// file and a new one.
pub fn renamed_file_diff(path: &Path, file: &Path, from: &Path) -> Result<String, String> {
  working_tree_patch(path, Some(file), Some(from))
}

/// Every staged and unstaged change against HEAD, including untracked files.
pub fn all_changes_diff(path: &Path) -> Result<String, String> {
  working_tree_patch(path, None, None)
}

/// Context lines that cover any file: a diff of one file carries the whole file, so that the interface can expand the unchanged lines
/// between and around the changes (it collapses them to a few lines at first).
const WHOLE_FILE: u32 = u32::MAX;

fn working_tree_patch(path: &Path, file: Option<&Path>, from: Option<&Path>) -> Result<String, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let head = repository.head().ok().and_then(|head| head.peel_to_tree().ok());
  let mut options = DiffOptions::new();
  options.include_untracked(true).show_untracked_content(true).recurse_untracked_dirs(true);
  if let Some(file) = file {
    options.pathspec(file).context_lines(WHOLE_FILE);
  }
  if let Some(from) = from {
    options.pathspec(from);
  }
  // With conflicts in the index, the index-aware diff has no content for a conflicted file; against the files on disk it shows the
  // conflict markers as added lines, which is what the user has to resolve.
  let conflicted = repository.index().is_ok_and(|index| index.has_conflicts());
  let mut diff = if conflicted {
    repository.diff_tree_to_workdir(head.as_ref(), Some(&mut options))
  } else {
    repository.diff_tree_to_workdir_with_index(head.as_ref(), Some(&mut options))
  }
  .map_err(|error| error.message().to_string())?;
  if from.is_some() {
    diff.find_similar(Some(git2::DiffFindOptions::new().renames(true).for_untracked(true))).map_err(|error| error.message().to_string())?;
  }
  patch_text(&diff)
}

/// A diff as the text of a unified patch.
fn patch_text(diff: &git2::Diff) -> Result<String, String> {
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

/// The absolute path of a file given relative to the repository containing `path`.
pub fn workdir_file(path: &Path, file: &Path) -> Option<PathBuf> {
  Repository::discover(path).ok()?.workdir().map(|workdir| workdir.join(file))
}

/// File contents in HEAD and the working tree, bounded per version for previews.
pub struct FileVersions {
  pub original: Option<Vec<u8>>,
  pub current: Option<Vec<u8>>,
}

/// Reads regular files relative to the repository root. Missing versions are returned as `None`.
pub fn file_versions(path: &Path, file: &Path, limit: usize) -> Result<FileVersions, String> {
  use std::{fs, io::Read};

  let repository = Repository::discover(path).map_err(|error| error.to_string())?;
  let workdir = repository.workdir().ok_or("The repository has no working folder.")?;
  let head = repository.head().ok().and_then(|head| head.peel_to_tree().ok());
  let original = head.as_ref().and_then(|tree| tree.get_path(file).ok()).filter(|entry| matches!(entry.filemode(), 0o100644 | 0o100755));
  let original = original
    .map(|entry| -> Result<_, String> {
      let odb = repository.odb().map_err(|error| error.to_string())?;
      let (size, _) = odb.read_header(entry.id()).map_err(|error| error.to_string())?;
      if size > limit {
        return Err("The original file is too large to preview.".into());
      }
      let blob = repository.find_blob(entry.id()).map_err(|error| error.to_string())?;
      Ok(blob.content().to_vec())
    })
    .transpose()?;

  let current_path = workdir.join(file);
  let current = match fs::symlink_metadata(&current_path) {
    Ok(metadata) if metadata.is_file() => {
      let mut bytes = Vec::new();
      fs::File::open(current_path).and_then(|file| file.take(limit as u64 + 1).read_to_end(&mut bytes)).map_err(|error| error.to_string())?;
      if bytes.len() > limit {
        return Err("The current file is too large to preview.".into());
      }
      Some(bytes)
    }
    Ok(_) => None,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
    Err(error) => return Err(error.to_string()),
  };
  Ok(FileVersions { original, current })
}

/// Throws away every change to one file (staged and unstaged) and cannot be undone. A file that is in the last commit
/// goes back to its committed content, also when it was deleted; a file that is not (a new one) is removed from the index
/// and from the disk. `file` is relative to the repository.
pub fn discard_changes(path: &Path, file: &Path) -> Result<(), String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  operation::require_idle(&repository)?;
  let workdir = repository.workdir().ok_or("The repository has no working folder.")?.to_path_buf();
  let committed = repository.head().ok().and_then(|head| head.peel_to_tree().ok()).is_some_and(|tree| tree.get_path(file).is_ok());
  if committed {
    let mut checkout = CheckoutBuilder::new();
    checkout.path(file).force();
    return repository.checkout_head(Some(&mut checkout)).map_err(|error| error.message().to_string());
  }
  let mut index = repository.index().map_err(|error| error.message().to_string())?;
  if index.get_path(file, 0).is_some() {
    index.remove_path(file).map_err(|error| error.message().to_string())?;
    index.write().map_err(|error| error.message().to_string())?;
  }
  match std::fs::remove_file(workdir.join(file)) {
    Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(format!("Could not remove {}: {error}", file.display())),
    _ => Ok(()),
  }
}

/// Commits `files` (relative to the repository) with the given message, and nothing else: other changes stay as they are,
/// also those that were staged. Returns the short id of the new commit.
///
/// The commit is built from the last commit plus the chosen files, so what was staged for other files is left out of it.
/// Afterwards the index of the chosen files matches the commit, like after `git commit <files>`.
pub fn commit(path: &Path, files: &[PathBuf], summary: &str, description: &str) -> Result<String, String> {
  build_commit(path, files, summary, description, false)
}

/// Replaces the last commit with one that also has `files` (which may be none, to only change the message), like
/// `git commit --amend`. A commit a remote branch already has cannot be amended: that would rewrite published history. Returns
/// the short id of the new commit.
pub fn amend_commit(path: &Path, files: &[PathBuf], summary: &str, description: &str) -> Result<String, String> {
  build_commit(path, files, summary, description, true)
}

fn build_commit(path: &Path, files: &[PathBuf], summary: &str, description: &str, amend: bool) -> Result<String, String> {
  let summary = summary.trim();
  if summary.is_empty() {
    return Err("Write a summary for the commit.".into());
  }
  if files.is_empty() && !amend {
    return Err("Choose at least one file to commit.".into());
  }
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  operation::require_idle(&repository)?;
  let workdir = repository.workdir().ok_or("The repository has no working folder.")?.to_path_buf();
  let signature = repository.signature().map_err(|_| "Set user.name and user.email in your Git configuration first.".to_string())?;
  let parent = repository.head().ok().and_then(|head| head.peel_to_commit().ok());
  if amend {
    let last = parent.as_ref().ok_or("There is no commit to amend.")?;
    if last.parent_count() > 1 {
      return Err("A merge commit cannot be amended here.".into());
    }
    if on_remote(&repository, last.id()) {
      return Err("This commit is already on the remote; amending it would rewrite published history.".into());
    }
  }

  // A copy of the last commit's tree in memory, with the chosen files applied.
  let mut tree_index = git2::Index::new().map_err(|error| error.message().to_string())?;
  if let Some(parent) = &parent {
    tree_index.read_tree(&parent.tree().map_err(|error| error.message().to_string())?).map_err(|error| error.message().to_string())?;
  }
  let mut real_index = repository.index().map_err(|error| error.message().to_string())?;
  for file in files {
    if workdir.join(file).exists() {
      real_index.add_path(file).map_err(|error| error.message().to_string())?;
      let entry = real_index.get_path(file, 0).ok_or_else(|| format!("Could not add {}.", file.display()))?;
      tree_index.add(&entry).map_err(|error| error.message().to_string())?;
    } else {
      // A deleted file: it leaves both the commit and the index.
      let _ = tree_index.remove_path(file);
      let _ = real_index.remove_path(file);
    }
  }
  let tree_id = tree_index.write_tree_to(&repository).map_err(|error| error.message().to_string())?;
  let tree = repository.find_tree(tree_id).map_err(|error| error.message().to_string())?;
  if !amend && parent.as_ref().is_some_and(|parent| parent.tree_id() == tree_id) {
    return Err("The chosen files have no changes to commit.".into());
  }
  let message = match description.trim() {
    "" => summary.to_string(),
    description => format!("{summary}\n\n{description}"),
  };
  let before_last: Option<git2::Commit> = if amend { parent.as_ref().and_then(|last| last.parent(0).ok()) } else { None };
  let parents: Vec<&git2::Commit> = if amend { before_last.iter().collect() } else { parent.iter().collect() };
  // An amended commit does not have the current tip as its parent, so the branch is moved to it afterwards.
  let id = repository
    .commit(if amend { None } else { Some("HEAD") }, &signature, &signature, &message, &tree, &parents)
    .map_err(|error| error.message().to_string())?;
  if amend {
    let head = repository.head().map_err(|error| error.message().to_string())?;
    if head.is_branch() {
      let name = head.name().map_err(|error| error.message().to_string())?.to_string();
      repository.reference(&name, id, true, "commit (amend)").map_err(|error| error.message().to_string())?;
    } else {
      repository.set_head_detached(id).map_err(|error| error.message().to_string())?;
    }
  }
  real_index.write().map_err(|error| error.message().to_string())?;
  Ok(id.to_string().chars().take(7).collect())
}

/// The local branches of the repository containing `path`, sorted by name (the current one is not moved to the front).
pub fn branches(path: &Path) -> Result<Vec<String>, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let mut names: Vec<String> = repository
    .branches(Some(git2::BranchType::Local))
    .map_err(|error| error.message().to_string())?
    .filter_map(Result::ok)
    .filter_map(|(branch, _)| branch.name().ok().flatten().map(str::to_string))
    .collect();
  names.sort_by_key(|name| name.to_lowercase());
  Ok(names)
}

/// Whether a branch called `name` (already trimmed) could be created now; gives the repository when it can.
pub fn check_new_branch(path: &Path, name: &str) -> Result<Repository, String> {
  if name.is_empty() {
    return Err("Enter a branch name.".into());
  }
  if !git2::Branch::name_is_valid(name).unwrap_or(false) {
    return Err(format!("\"{name}\" is not a valid branch name."));
  }
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  operation::require_idle(&repository)?;
  if repository.find_branch(name, git2::BranchType::Local).is_ok() {
    return Err(format!("A branch named {name} already exists."));
  }
  Ok(repository)
}

/// Creates a branch at the current commit and switches to it, like `git switch -c`. The changes in the working tree come along.
/// In a repository without commits the branch just becomes the name of the unborn one.
pub fn create_branch(path: &Path, name: &str) -> Result<(), String> {
  let name = name.trim();
  let repository = check_new_branch(path, name)?;
  match repository.head().ok().and_then(|head| head.peel_to_commit().ok()) {
    Some(commit) => {
      repository.branch(name, &commit, false).map_err(|error| error.message().to_string())?;
    }
    None => return repository.set_head(&format!("refs/heads/{name}")).map_err(|error| error.message().to_string()),
  }
  repository.set_head(&format!("refs/heads/{name}")).map_err(|error| error.message().to_string())
}

/// Switches to an existing local branch, like `git switch`. Changes that the switch would overwrite stop it: nothing is
/// forced, nothing is lost.
pub fn switch_branch(path: &Path, name: &str) -> Result<(), String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  operation::require_idle(&repository)?;
  // A branch only a remote has gets a local branch that follows it, like `git switch` does.
  if repository.find_branch(name, git2::BranchType::Local).is_err() {
    branch::track_remote_branch(&repository, name)?;
  }
  let branch = repository.find_branch(name, git2::BranchType::Local).map_err(|_| format!("There is no branch named {name}."))?;
  let commit = branch.get().peel_to_commit().map_err(|error| error.message().to_string())?;
  let mut checkout = CheckoutBuilder::new();
  checkout.safe();
  repository.checkout_tree(commit.as_object(), Some(&mut checkout)).map_err(|error| {
    if error.code() == git2::ErrorCode::Conflict {
      format!("{SWITCH_OVERWRITE} {name}. Commit or discard them first.")
    } else {
      error.message().to_string()
    }
  })?;
  repository.set_head(&format!("refs/heads/{name}")).map_err(|error| error.message().to_string())
}

/// The unified diff of several files against the last commit, in one pass over the repository (one pass per file would scan
/// the whole working tree every time, which takes minutes for a thousand files). `files` are relative to the repository and
/// match literally, so a path such as `app/[id]/page.tsx` is not read as a pattern.
pub fn diff_files(path: &Path, files: &[PathBuf]) -> Result<String, String> {
  if files.is_empty() {
    return Ok(String::new());
  }
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let head = repository.head().ok().and_then(|head| head.peel_to_tree().ok());
  let mut options = DiffOptions::new();
  options.include_untracked(true).show_untracked_content(true).recurse_untracked_dirs(true).disable_pathspec_match(true);
  for file in files {
    options.pathspec(file);
  }
  let diff = repository.diff_tree_to_workdir_with_index(head.as_ref(), Some(&mut options)).map_err(|error| error.message().to_string())?;
  patch_text(&diff)
}

/// The Git command as the activity log shows it: the repository and the arguments, never credentials.
pub(crate) fn describe_git(path: &Path, arguments: &[&str]) -> String {
  format!("git -C {} {}", path.display(), arguments.join(" "))
}

/// How a finished Git command ended, for the activity log.
pub(crate) fn describe_output(output: &Result<std::process::Output, String>) -> String {
  match output {
    Ok(output) if output.status.success() => "ok".into(),
    Ok(output) => format!("failed ({}): {}", output.status, last_line(&String::from_utf8_lossy(&output.stderr))),
    Err(error) => format!("not run: {error}"),
  }
}

/// A user name and a password (an access token) for Git over HTTPS, given to `git` for one command through a credential
/// helper that reads them from the environment, so they never appear in a command line or a file.
#[derive(Clone)]
pub struct Credentials {
  pub username: String,
  pub password: String,
}

impl std::fmt::Debug for Credentials {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.debug_struct("Credentials").field("username", &self.username).field("password", &"…").finish()
  }
}

/// The credential helper `git` is given: it answers a `get` with the user name and password from the environment.
pub const CREDENTIAL_HELPER: &str =
  "!f() { test \"$1\" = get && printf 'username=%s\\npassword=%s\\n' \"$WS_GIT_USERNAME\" \"$WS_GIT_PASSWORD\"; }; f";

/// The options that make `git` use `credentials` for the command: the user's own helpers are replaced for this command only.
fn credential_options(command: &mut std::process::Command, credentials: Option<&Credentials>) {
  let Some(credentials) = credentials else { return };
  command
    .args(["-c", "credential.helper=", "-c"])
    .arg(format!("credential.helper={CREDENTIAL_HELPER}"))
    .env("WS_GIT_USERNAME", &credentials.username)
    .env("WS_GIT_PASSWORD", &credentials.password);
}

/// Runs the `git` program in `path` with `--progress` and hands each line it reports (git redraws progress lines with a carriage
/// return) to `on_progress` while it runs. It never asks for input, so the user's own credentials apply and a login prompt becomes
/// git's error message. Returns whether it succeeded and the combined output from git and its hooks.
fn run_git(path: &Path, arguments: &[&str], credentials: Option<&Credentials>, on_progress: impl FnMut(&str)) -> Result<(bool, String), String> {
  ws_activity::timed(
    "git",
    || describe_git(path, arguments),
    || run_git_untimed(path, arguments, credentials, on_progress),
    |result| match result {
      Ok((true, _)) => "ok".into(),
      Ok((false, output)) => format!("failed: {}", last_line(output)),
      Err(error) => format!("not run: {error}"),
    },
  )
}

fn run_git_untimed(
  path: &Path,
  arguments: &[&str],
  credentials: Option<&Credentials>,
  mut on_progress: impl FnMut(&str),
) -> Result<(bool, String), String> {
  use std::io::Read as _;
  let mut command = std::process::Command::new("git");
  shell_path::apply(&mut command);
  command.arg("-C").arg(path);
  credential_options(&mut command, credentials);
  // Some hooks print their diagnostics to stdout. A shared pipe streams both channels without blocking either one.
  let (mut output, writer) = std::io::pipe().map_err(|error| format!("Could not capture git's output: {error}"))?;
  let stdout = writer.try_clone().map_err(|error| format!("Could not capture git's output: {error}"))?;
  let mut child = command
    .args(arguments)
    .arg("--progress")
    .env("GIT_TERMINAL_PROMPT", "0")
    .stdin(std::process::Stdio::null())
    .stdout(stdout)
    .stderr(writer)
    .spawn()
    .map_err(|error| format!("Could not run git: {error}"))?;
  drop(command);
  let (mut all, mut line, mut buffer) = (Vec::new(), Vec::new(), [0u8; 512]);
  loop {
    let read = output.read(&mut buffer).map_err(|error| format!("Could not read git's output: {error}"))?;
    if read == 0 {
      break;
    }
    all.extend_from_slice(&buffer[..read]);
    for byte in &buffer[..read] {
      if matches!(byte, b'\r' | b'\n') {
        let text = String::from_utf8_lossy(&line).trim().to_string();
        if !text.is_empty() {
          on_progress(&text);
        }
        line.clear();
      } else {
        line.push(*byte);
      }
    }
  }
  let rest = String::from_utf8_lossy(&line);
  if !rest.trim().is_empty() {
    on_progress(rest.trim());
  }
  let status = child.wait().map_err(|error| format!("Could not run git: {error}"))?;
  Ok((status.success(), String::from_utf8_lossy(&all).to_string()))
}

/// The last line git wrote, ignoring the redrawn progress states (`\r`-separated) before it.
fn last_line(text: &str) -> String {
  text.split(['\r', '\n']).rev().find(|line| !line.trim().is_empty()).unwrap_or("").trim().to_string()
}

/// What the message of a failed push says when the server accepted the login but refused commits that change workflow files
/// (GitHub does so unless the token may change workflows): the interface looks for it to mark the account as having a problem.
pub const WORKFLOWS_REFUSED: &str = "Workflows: Read and write";

/// Whether a line is git's progress report ("Resolving deltas:  43% (20/46)", "Counting objects: …"), not something to tell the user.
fn is_progress(line: &str) -> bool {
  const STEPS: [&str; 7] =
    ["Resolving deltas", "Counting objects", "Compressing objects", "Enumerating objects", "Writing objects", "Delta compression", "Total "];
  STEPS.iter().any(|step| line.starts_with(step)) || line.contains("% (")
}

/// A hook (Husky, for one) that stopped because a program it runs is not found: `.husky/pre-push: line 1: bun: command not found`.
fn missing_hook_command(stderr: &str) -> Option<String> {
  stderr.lines().find_map(|line| {
    let (hook, rest) = line.trim().split_once(": line ")?;
    let (_, rest) = rest.split_once(": ")?;
    let program = rest.strip_suffix(": command not found")?;
    let name = hook.rsplit('/').next().unwrap_or(hook);
    Some(format!(
      "The {name} hook could not run `{program}`: it is not installed or not on your PATH. Install it, or add it to your login shell's PATH (for example in ~/.zprofile), then try again."
    ))
  })
}

/// Why a git command failed, in a sentence. git ends its output with a line such as "failed to push some refs", which says nothing;
/// the reason is in the line about the rejected branch or in what the remote said.
fn failure_reason(stderr: &str, failure: &str) -> String {
  if let Some(message) = missing_hook_command(stderr) {
    return message;
  }
  let lines: Vec<&str> = stderr.split(['\r', '\n']).map(str::trim).filter(|line| !line.is_empty()).collect();
  let mut message = match lines.iter().find(|line| line.contains("[remote rejected]") || line.contains("[rejected]")) {
    Some(rejected) => {
      // The remote's own words, when it said any, come with the rejection.
      let mut said: Vec<&str> =
        lines.iter().filter_map(|line| line.strip_prefix("remote:")).map(str::trim).filter(|line| !line.is_empty() && !is_progress(line)).collect();
      said.dedup();
      let rejected = rejected.trim_start_matches('!').trim();
      if said.is_empty() {
        rejected.to_string()
      } else {
        format!("{rejected}: {}", said.join(" "))
      }
    }
    None => {
      // git's closing "failed to push some refs" says nothing; what comes before it (the remote's words, `fatal:` and `error:` lines) does.
      let quiet: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| !line.starts_with("error: failed to push some refs") && !line.starts_with("hint:") && !is_progress(line))
        .collect();
      let mut details: Vec<&str> = quiet
        .iter()
        .copied()
        .filter(|line| line.starts_with("remote:") || line.starts_with("fatal:") || line.starts_with("error:"))
        .map(|line| line.strip_prefix("remote:").map_or(line, str::trim))
        .filter(|line| !line.is_empty())
        .collect();
      // Nothing from git or the remote: a hook (pre-push) turned the push down, and what it printed is the reason.
      if details.is_empty() {
        details = quiet.iter().rev().take(6).rev().copied().collect();
      }
      details.dedup();
      match last_line(stderr) {
        line if line.is_empty() => failure.to_string(),
        line if details.is_empty() => line,
        _ => details.join(" "),
      }
    }
  };
  let lowered = stderr.to_lowercase();
  if lowered.contains("workflow") && (lowered.contains("scope") || lowered.contains("permission")) {
    message.push_str(&format!(
      " The token needs the {WORKFLOWS_REFUSED} permission (a classic token, the `workflow` scope), because the commits change files in .github/workflows."
    ));
  }
  message
}

fn finish(result: (bool, String), failure: &str) -> Result<String, String> {
  let (success, output) = result;
  if success {
    return Ok(last_line(&output));
  }
  Err(failure_reason(&output, failure))
}

/// What the message of a failed push, fetch or pull says when the server turned down the connected account's token (and the user's own
/// login could not be used either): the interface looks for it to mark the account as having a problem.
/// What the message says when no login could be given to Git at all: no account serves the address, or its token is missing
/// from the Keychain.
pub const NO_LOGIN: &str = "The app had no login to give Git";

pub const TOKEN_REFUSED: &str = "refused the token of the connected account";

/// Whether git's message says the server turned the login down (rather than, say, a network failure).
fn login_refused(message: &str) -> bool {
  ["403", "401", "Authentication failed", "denied", "could not read Username", "could not read Password", "Permission to"]
    .iter()
    .any(|sign| message.contains(sign))
}

/// Runs a command that talks to the remote with the connected account's token; if the server refuses that login, it is run
/// once more with the user's own Git login (SSH key, credential helper), which is what happened before an account was
/// connected. When both are refused, the message says so.
fn with_login(credentials: Option<&Credentials>, mut command: impl FnMut(Option<&Credentials>) -> Result<String, String>) -> Result<String, String> {
  match (command(credentials), credentials) {
    (Err(first), Some(_)) if login_refused(&first) => command(None).map_err(|second| {
      if login_refused(&second) {
        format!(
          "The server {TOKEN_REFUSED} ({}). Your own Git login could not be used either ({}). Give the token write access in Settings → Accounts (reconnect the account with a new token), or sign in once with `git push` in a terminal.",
          first.trim_end_matches('.'),
          second.trim_end_matches('.')
        )
      } else {
        second
      }
    }),
    // Nothing to give git: no account serves the address, or its token is not in the Keychain. git's own words ("could not read
    // Username") do not say what to do about it.
    (Err(error), None) if login_refused(&error) => Err(format!(
      "{} {NO_LOGIN} (no account is connected for this address, or the account's token is missing from the Keychain), and your own Git login has no saved password. Connect the account again in Settings → Accounts, or sign in once with `git push` in a terminal.",
      error.trim_end()
    )),
    (result, _) => result,
  }
}

/// Pushes the current branch with the `git` program, so the user's own credentials apply; a branch without an upstream is
/// published to `origin`. `on_progress` gets git's progress lines ("Writing objects: 45% …") while it runs. With `credentials`
/// (a connected account's token) they are tried first, and the user's own login after a refusal.
pub fn push(path: &Path, credentials: Option<&Credentials>, mut on_progress: impl FnMut(&str)) -> Result<String, String> {
  with_login(credentials, |credentials| {
    let mut result = run_git(path, &["push"], credentials, &mut on_progress)?;
    if !result.0 && result.1.contains("no upstream branch") {
      result = run_git(path, &["push", "--set-upstream", "origin", "HEAD"], credentials, &mut on_progress)?;
    }
    finish(result, "git push failed.")
  })
}

/// Fetches the remote of the current branch (`git fetch`). Changes nothing in the working tree.
pub fn fetch(path: &Path, credentials: Option<&Credentials>, mut on_progress: impl FnMut(&str)) -> Result<String, String> {
  with_login(credentials, |credentials| finish(run_git(path, &["fetch"], credentials, &mut on_progress)?, "git fetch failed."))
}

/// Brings in the remote's new commits with `git pull`: a branch that only fell behind moves forward, and one
/// that has commits of its own gets a merge commit. The user's `pull.rebase` setting is honored; without one the pull merges,
/// instead of asking git to choose. A merge that conflicts stays in progress (see [`read_merge`]) for the conflict resolver, and
/// the error says so.
pub fn pull(path: &Path, credentials: Option<&Credentials>, mut on_progress: impl FnMut(&str)) -> Result<String, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  operation::require_idle(&repository)?;
  let rebase_configured = repository.config().is_ok_and(|config| config.get_entry("pull.rebase").is_ok());
  let arguments: &[&str] = if rebase_configured { &["pull"] } else { &["pull", "--no-rebase", "--no-edit"] };
  with_login(credentials, |credentials| {
    let result = run_git(path, arguments, credentials, &mut on_progress)?;
    // git reports the conflicting files on its standard output, which is not read: the state of the repository says it.
    if !result.0 {
      match Operation::read(&repository) {
        Operation::Merge => return Err("The pull has merge conflicts. Resolve them to finish the merge, or abort it.".into()),
        Operation::Rebase => {
          return Err("The pull stopped during rebase. Resolve and stage the conflicts, then continue or abort the rebase.".into())
        }
        _ => {}
      }
    }
    finish(result, "git pull failed.")
  })
}

/// Whether a remote branch already has the commit.
fn on_remote(repository: &Repository, id: git2::Oid) -> bool {
  let Ok(references) = repository.references_glob("refs/remotes/*") else { return false };
  references
    .filter_map(Result::ok)
    .filter_map(|reference| reference.target())
    .any(|tip| tip == id || repository.graph_descendant_of(tip, id).unwrap_or(false))
}

/// Takes the last commit back, like `git reset --soft HEAD~1`: the branch goes to the commit before it, and the files and the
/// index stay as they are, so its changes are back in the list of changes. Returns the message it had, as (summary, description),
/// to put back in the commit box. Only the commit HEAD is on can be undone, not a merge or the first commit, and not one a remote
/// branch already has: that would rewrite published history.
pub fn undo_commit(path: &Path, id: &str) -> Result<(String, String), String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  operation::require_idle(&repository)?;
  let commit = repository.head().ok().and_then(|head| head.peel_to_commit().ok()).ok_or("There is no commit to undo.")?;
  if commit.id().to_string() != id {
    return Err("Only the last commit of the branch can be undone.".into());
  }
  if commit.parent_count() != 1 {
    return Err(
      if commit.parent_count() == 0 { "The first commit of a repository cannot be undone." } else { "A merge commit cannot be undone here." }.into(),
    );
  }
  if on_remote(&repository, commit.id()) {
    return Err("This commit is already on the remote; undoing it would rewrite published history.".into());
  }
  let parent = commit.parent(0).map_err(|error| error.message().to_string())?;
  repository.reset(parent.as_object(), git2::ResetType::Soft, None).map_err(|error| error.message().to_string())?;
  let message = commit.message().unwrap_or("");
  let (summary, rest) = message.split_once('\n').unwrap_or((message, ""));
  Ok((summary.trim().to_string(), rest.trim().to_string()))
}

/// Creates a lightweight tag with this name on a commit. The name must be valid and not taken.
pub fn create_tag(path: &Path, id: &str, name: &str) -> Result<(), String> {
  let name = name.trim();
  if name.is_empty() {
    return Err("Enter a tag name.".into());
  }
  if !git2::Reference::is_valid_name(&format!("refs/tags/{name}")) {
    return Err(format!("\"{name}\" is not a valid tag name."));
  }
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  if repository.find_reference(&format!("refs/tags/{name}")).is_ok() {
    return Err(format!("A tag named {name} already exists."));
  }
  let object = repository
    .find_object(git2::Oid::from_str(id).map_err(|error| error.message().to_string())?, None)
    .map_err(|error| error.message().to_string())?;
  repository.tag_lightweight(name, &object, false).map(|_| ()).map_err(|error| error.message().to_string())
}

/// The name of the remote the current branch follows, else `origin`, else the first remote; `None` without remotes.
fn main_remote_name(repository: &Repository) -> Option<String> {
  let remotes = repository.remotes().ok()?;
  let followed = repository
    .head()
    .ok()
    .and_then(|head| head.name().ok().map(str::to_string))
    .and_then(|name| repository.branch_upstream_remote(&name).ok())
    .and_then(|remote| remote.as_str().ok().map(str::to_string));
  followed
    .or_else(|| remotes.iter().flatten().flatten().find(|name| *name == "origin").map(str::to_string))
    .or_else(|| remotes.iter().flatten().flatten().next().map(str::to_string))
}

/// The URL of the remote the current branch follows, else `origin`, else the first remote; `None` without remotes.
pub fn remote_url(path: &Path) -> Option<String> {
  let repository = Repository::discover(path).ok()?;
  let name = main_remote_name(&repository)?;
  let remote = repository.find_remote(&name).ok()?;
  remote.url().ok().map(str::to_string)
}

/// Points the remote that [`remote_url`] reports at `url` (a repository that was renamed or moved), or adds `origin` when there is
/// no remote yet. Branches keep following the same remote, so nothing else changes. Fetching afterwards brings in what is there.
pub fn set_remote_url(path: &Path, url: &str) -> Result<(), String> {
  let url = url.trim();
  if url.is_empty() || url.chars().any(char::is_whitespace) {
    return Err("The remote address must not be empty or contain spaces.".into());
  }
  let repository = Repository::discover(path).map_err(|_| "This folder is not a Git repository.".to_string())?;
  match main_remote_name(&repository) {
    Some(name) => repository.remote_set_url(&name, url),
    None => repository.remote("origin", url).map(|_| ()),
  }
  .map_err(|error| error.message().to_string())
}

/// A cheap stamp of what the History view lists: the commit HEAD is at and the one its upstream is at. It changes when a commit is
/// made, a branch is switched, or a fetch or push moves the upstream, and not otherwise, so the list is read again only then.
pub fn history_stamp(path: &Path) -> Option<String> {
  let repository = Repository::discover(path).ok()?;
  let head = repository.head().ok();
  let target = head.as_ref().and_then(|head| head.target()).map(|id| id.to_string()).unwrap_or_default();
  let upstream = head
    .as_ref()
    .and_then(|head| head.shorthand().ok().map(str::to_string))
    .and_then(|name| repository.find_branch(&name, git2::BranchType::Local).ok())
    .and_then(|branch| branch.upstream().ok())
    .and_then(|upstream| upstream.get().target())
    .map(|id| id.to_string())
    .unwrap_or_default();
  Some(format!("{target}:{upstream}"))
}

/// The first line of the last commit's message.
pub fn head_summary(path: &Path) -> Option<String> {
  let repository = Repository::discover(path).ok()?;
  let commit = repository.head().ok()?.peel_to_commit().ok()?;
  commit.summary().ok().flatten().map(str::to_string)
}

/// People who made recent commits, as `(name, email)`, the person committing excluded: the suggestions for a co-author. Scans at
/// most `scan` commits and returns at most `limit` people, most recent first.
pub fn recent_authors(path: &Path, scan: usize, limit: usize) -> Vec<(String, String)> {
  let Ok(repository) = Repository::discover(path) else { return Vec::new() };
  let me = repository.signature().ok().and_then(|signature| signature.email().ok().map(str::to_lowercase));
  let Ok(mut walk) = repository.revwalk() else { return Vec::new() };
  if walk.push_head().is_err() {
    return Vec::new();
  }
  let mut seen = std::collections::HashSet::new();
  let mut found = Vec::new();
  for id in walk.filter_map(Result::ok).take(scan) {
    let Ok(commit) = repository.find_commit(id) else { continue };
    let author = commit.author();
    let (Ok(name), Ok(email)) = (author.name(), author.email()) else { continue };
    let key = email.to_lowercase();
    if name.is_empty() || email.is_empty() || Some(&key) == me.as_ref() || !seen.insert(key) {
      continue;
    }
    found.push((name.to_string(), email.to_string()));
    if found.len() >= limit {
      break;
    }
  }
  found
}

/// The summary and the description of the last commit, to fill the commit box when amending.
pub fn head_message(path: &Path) -> Option<(String, String)> {
  let repository = Repository::discover(path).ok()?;
  let commit = repository.head().ok()?.peel_to_commit().ok()?;
  let message = commit.message().ok()?;
  let (summary, rest) = message.split_once('\n').unwrap_or((message, ""));
  Some((summary.trim().to_string(), rest.trim().to_string()))
}

/// A commit in the history list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitInfo {
  /// The full id.
  pub id: String,
  /// The first seven characters of the id.
  pub short: String,
  /// The first line of the message.
  pub summary: String,
  pub author: String,
  /// When it was made, in seconds since 1970.
  pub time: i64,
  /// Not on the remote yet: the branch's upstream does not have it (or, without an upstream, no remote branch does).
  pub unpushed: bool,
  /// The tags that point at it.
  pub tags: Vec<String>,
}

/// A commit with what it changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitDetail {
  pub info: CommitInfo,
  /// The message after its first line.
  pub body: String,
  /// The author's e-mail address, empty when the commit has none.
  pub author_email: String,
  /// The files it changed, sorted by path (against its first parent; a first commit against nothing).
  pub files: Vec<Change>,
  pub additions: usize,
  pub deletions: usize,
}

fn commit_info(
  commit: &git2::Commit,
  unpushed: &std::collections::HashSet<git2::Oid>,
  tags: &std::collections::HashMap<git2::Oid, Vec<String>>,
) -> CommitInfo {
  let id = commit.id().to_string();
  CommitInfo {
    short: id.chars().take(7).collect(),
    id,
    summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
    author: commit.author().name().unwrap_or("").to_string(),
    time: commit.time().seconds(),
    unpushed: unpushed.contains(&commit.id()),
    tags: tags.get(&commit.id()).cloned().unwrap_or_default(),
  }
}

/// The last `limit` commits reachable from HEAD, newest first, with which of them are not on the remote yet and their tags.
pub fn history(path: &Path, limit: usize) -> Result<Vec<CommitInfo>, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  if repository.head().is_err() {
    return Ok(Vec::new());
  }
  // Commits the remote does not have: from HEAD without what the upstream (else every remote branch) reaches.
  let mut unpushed = std::collections::HashSet::new();
  if repository.remotes().map(|remotes| !remotes.is_empty()).unwrap_or(false) {
    let mut walk = repository.revwalk().map_err(|error| error.message().to_string())?;
    walk.push_head().map_err(|error| error.message().to_string())?;
    let upstream = repository
      .head()
      .ok()
      .and_then(|head| head.shorthand().ok().map(str::to_string))
      .and_then(|name| repository.find_branch(&name, git2::BranchType::Local).ok())
      .and_then(|branch| branch.upstream().ok())
      .and_then(|upstream| upstream.get().target());
    match upstream {
      Some(target) => {
        let _ = walk.hide(target);
      }
      None => {
        let _ = walk.hide_glob("refs/remotes/*");
      }
    }
    unpushed.extend(walk.filter_map(Result::ok));
  }
  let mut tags: std::collections::HashMap<git2::Oid, Vec<String>> = std::collections::HashMap::new();
  let _ = repository.tag_foreach(|id, name| {
    if let Some(commit) = repository.find_object(id, None).ok().and_then(|object| object.peel_to_commit().ok()) {
      tags.entry(commit.id()).or_default().push(String::from_utf8_lossy(name).trim_start_matches("refs/tags/").to_string());
    }
    true
  });
  let mut walk = repository.revwalk().map_err(|error| error.message().to_string())?;
  walk.set_sorting(git2::Sort::TIME).map_err(|error| error.message().to_string())?;
  walk.push_head().map_err(|error| error.message().to_string())?;
  Ok(
    walk
      .filter_map(Result::ok)
      .filter_map(|id| repository.find_commit(id).ok())
      .take(limit)
      .map(|commit| commit_info(&commit, &unpushed, &tags))
      .collect(),
  )
}

/// Returns the most recent commits that changed the selected path, starting at HEAD.
pub fn file_history(path: &Path, limit: usize) -> Result<Vec<CommitInfo>, String> {
  let result = |error: git2::Error| error.message().to_owned();
  let repository = Repository::discover(path.parent().ok_or("The file has no parent folder.")?).map_err(result)?;
  let workdir = repository.workdir().ok_or("The repository has no working tree.")?;
  let parent = path.parent().ok_or("The file has no parent folder.")?;
  let parent = parent.canonicalize().map_err(|error| error.to_string())?;
  let relative = parent.strip_prefix(workdir).map_err(|_| "The file is outside the Git working tree.")?;
  let relative = relative.join(path.file_name().ok_or("The file has no name.")?);
  match repository.head() {
    Ok(_) => {}
    Err(error) if matches!(error.code(), git2::ErrorCode::UnbornBranch | git2::ErrorCode::NotFound) => return Ok(Vec::new()),
    Err(error) => return Err(result(error)),
  }

  let mut walk = repository.revwalk().map_err(result)?;
  walk.set_sorting(git2::Sort::TIME).map_err(result)?;
  walk.push_head().map_err(result)?;
  let entry = |commit: &git2::Commit| -> Result<Option<(git2::Oid, i32)>, String> {
    let tree = commit.tree().map_err(result)?;
    Ok(tree.get_path(&relative).ok().map(|entry| (entry.id(), entry.filemode())))
  };
  let mut history = Vec::new();
  for id in walk {
    if history.len() == limit {
      break;
    }
    let commit = repository.find_commit(id.map_err(result)?).map_err(result)?;
    let previous = match commit.parent(0) {
      Ok(parent) => entry(&parent)?,
      Err(_) => None,
    };
    if entry(&commit)? != previous {
      history.push(commit_info(&commit, &Default::default(), &Default::default()));
    }
  }
  Ok(history)
}

fn commit_diff<'a>(repository: &'a Repository, commit: &git2::Commit, file: Option<&Path>) -> Result<git2::Diff<'a>, String> {
  let tree = commit.tree().map_err(|error| error.message().to_string())?;
  let parent = commit.parent(0).ok().and_then(|parent| parent.tree().ok());
  let mut options = DiffOptions::new();
  if let Some(file) = file {
    options.pathspec(file).context_lines(WHOLE_FILE);
  }
  repository.diff_tree_to_tree(parent.as_ref(), Some(&tree), Some(&mut options)).map_err(|error| error.message().to_string())
}

/// What a commit says and changed.
pub fn commit_detail(path: &Path, id: &str) -> Result<CommitDetail, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let commit =
    repository.find_commit(git2::Oid::from_str(id).map_err(|error| error.message().to_string())?).map_err(|error| error.message().to_string())?;
  let diff = commit_diff(&repository, &commit, None)?;
  let mut files: Vec<Change> = diff
    .deltas()
    .filter_map(|delta| {
      let kind = match delta.status() {
        git2::Delta::Added | git2::Delta::Untracked => ChangeKind::New,
        git2::Delta::Deleted => ChangeKind::Deleted,
        git2::Delta::Renamed => ChangeKind::Renamed,
        git2::Delta::Conflicted => ChangeKind::Conflicted,
        _ => ChangeKind::Modified,
      };
      let file = if kind == ChangeKind::Deleted { delta.old_file().path() } else { delta.new_file().path() }?;
      Some(Change { path: file.to_path_buf(), kind, from: None })
    })
    .collect();
  files.sort_by(|a, b| a.path.cmp(&b.path));
  let stats = diff.stats().map_err(|error| error.message().to_string())?;
  let message = commit.message().unwrap_or("");
  let body = message.split_once('\n').map(|(_, rest)| rest.trim().to_string()).unwrap_or_default();
  let author = commit.author();
  let author_email = author.email().unwrap_or_default().to_string();
  Ok(CommitDetail {
    info: commit_info(&commit, &std::collections::HashSet::new(), &std::collections::HashMap::new()),
    body,
    author_email,
    files,
    additions: stats.insertions(),
    deletions: stats.deletions(),
  })
}

/// The unified diff of one file in a commit, against its first parent, with the whole file as context. `file` is relative to the repository.
pub fn commit_file_diff(path: &Path, id: &str, file: &Path) -> Result<String, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let commit =
    repository.find_commit(git2::Oid::from_str(id).map_err(|error| error.message().to_string())?).map_err(|error| error.message().to_string())?;
  let diff = commit_diff(&repository, &commit, Some(file))?;
  patch_text(&diff)
}

/// The folders directly inside `folder` that are Git repositories, sorted by name. Folders that cannot be read are skipped.
pub fn find_repositories(folder: &Path) -> Vec<PathBuf> {
  let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
  let mut found: Vec<PathBuf> = entries.filter_map(Result::ok).map(|entry| entry.path()).filter(|path| path.join(".git").exists()).collect();
  found.sort_by_key(|path| path.file_name().map(|name| name.to_string_lossy().to_lowercase()));
  found
}
