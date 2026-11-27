//! Management of native Git linked worktrees.

use crate::commit_actions::{failure, git};
use git2::Repository;
use std::{
  ffi::{OsStr, OsString},
  os::unix::ffi::OsStringExt,
  path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
  pub path: PathBuf,
  pub branch: Option<String>,
  pub main: bool,
  pub locked: bool,
  pub prunable: bool,
}

pub fn worktrees(root: &Path) -> Result<Vec<Worktree>, String> {
  let output = git(root, &["worktree", "list", "--porcelain", "-z"])?;
  if !output.status.success() {
    return Err(failure(&output, "Git could not list worktrees."));
  }
  let mut trees: Vec<Worktree> = Vec::new();
  for field in output.stdout.split(|byte| *byte == 0) {
    if let Some(path) = field.strip_prefix(b"worktree ") {
      trees.push(Worktree { path: OsString::from_vec(path.to_vec()).into(), branch: None, main: trees.is_empty(), locked: false, prunable: false });
    } else if let Some(tree) = trees.last_mut() {
      if let Some(branch) = field.strip_prefix(b"branch refs/heads/") {
        tree.branch = Some(String::from_utf8_lossy(branch).into_owned());
      }
      tree.locked |= field == b"locked" || field.starts_with(b"locked ");
      tree.prunable |= field == b"prunable" || field.starts_with(b"prunable ");
    }
  }
  Ok(trees)
}

pub(crate) fn require_branch_available(repository: &Repository, branch: &str) -> Result<(), String> {
  if !repository.is_worktree() && repository.worktrees().map_err(|error| error.to_string())?.is_empty() {
    return Ok(());
  }
  let active = repository.workdir().ok_or("The repository has no working folder.")?.canonicalize().map_err(|error| error.to_string())?;
  for tree in worktrees(&active)? {
    if tree.branch.as_deref() == Some(branch) && tree.path.canonicalize().ok().as_ref() != Some(&active) {
      return Err(format!("{branch} is checked out in {}. Open that worktree to use it.", tree.path.display()));
    }
  }
  Ok(())
}

pub fn add_worktree(root: &Path, path: &Path, branch: &str) -> Result<PathBuf, String> {
  if !path.is_absolute() || path.file_name().is_none() {
    return Err("Enter an absolute path to a new worktree folder.".into());
  }
  if path.symlink_metadata().is_ok() {
    return Err("The worktree folder already exists. Choose a new folder.".into());
  }
  if !git2::Branch::name_is_valid(branch).unwrap_or(false) || branch.starts_with('-') {
    return Err("Enter a valid local branch name.".into());
  }
  let repository = Repository::discover(root).map_err(|error| error.to_string())?;
  crate::operation::require_idle(&repository)?;
  let exists = match repository.find_branch(branch, git2::BranchType::Local) {
    Ok(_) => true,
    Err(error) if error.code() == git2::ErrorCode::NotFound => false,
    Err(error) => return Err(error.to_string()),
  };
  let mut arguments: Vec<&OsStr> = vec![OsStr::new("worktree"), OsStr::new("add"), OsStr::new("--no-guess-remote")];
  if !exists {
    arguments.extend([OsStr::new("-b"), OsStr::new(branch)]);
  }
  arguments.extend([OsStr::new("--"), path.as_os_str(), OsStr::new(if exists { branch } else { "HEAD" })]);
  let output = git(root, &arguments)?;
  if !output.status.success() {
    return Err(failure(&output, "Git could not create the worktree."));
  }
  Ok(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}

pub fn remove_worktree(root: &Path, expected: &Worktree) -> Result<(), String> {
  let repository = Repository::discover(root).map_err(|error| error.to_string())?;
  let active = repository.workdir().ok_or("The repository has no working folder.")?;
  let trees = worktrees(root)?;
  let tree = trees.iter().find(|tree| *tree == expected).ok_or("The worktree changed. Refresh the list and try again.")?;
  if tree.main
    || tree.locked
    || tree.prunable
    || active.canonicalize().map_err(|error| error.to_string())? == tree.path.canonicalize().map_err(|error| error.to_string())?
  {
    return Err("Only an available, unlocked linked worktree outside the current project can be removed.".into());
  }
  let linked = Repository::open(&tree.path).map_err(|error| error.to_string())?;
  crate::operation::require_idle(&linked)?;
  // Include ignored files so generated or private local files cannot disappear with the folder.
  let status = git(&tree.path, &["status", "--porcelain", "--ignored", "--untracked-files=normal"])?;
  if !status.status.success() {
    return Err(failure(&status, "Git could not check the worktree before removing it."));
  }
  if !status.stdout.is_empty() {
    return Err("Move, commit, or clean up local files in the worktree before removing it.".into());
  }
  let output = git(root, &["worktree".as_ref(), "remove".as_ref(), "--".as_ref(), tree.path.as_os_str()])?;
  if !output.status.success() {
    return Err(failure(&output, "Git could not remove the worktree."));
  }
  Ok(())
}
