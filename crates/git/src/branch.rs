//! Branches: remote branches next to the local ones, deleting and renaming, and what happens to
//! uncommitted changes when switching.

use super::Credentials;
use git2::{BranchType, Repository, RepositoryState};
use std::{
  collections::HashSet,
  path::Path,
  process::{Command, Output, Stdio},
};

/// What `switch_branch` says when the changes in the working tree are in the way: the interface offers to set them aside.
pub const SWITCH_OVERWRITE: &str = "Your changes would be overwritten by switching to";

/// The marker of the stash entries the app makes, followed by the branch the changes belong to.
const STASH_MARK: &str = "!!app-stash<";

/// A branch the branch menu lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchEntry {
  /// The branch name without a remote (`feature`, not `origin/feature`).
  pub name: String,
  /// The remote that has this branch when there is no local branch of the name yet; switching creates the local one.
  pub remote: Option<String>,
  /// When the branch's latest commit was made, in seconds since the epoch: the last time work landed on it.
  pub time: Option<i64>,
  /// The repository's default branch: the one the remote's `HEAD` points at (else `main`, `master` or `develop`, whichever exists).
  pub default: bool,
}

/// What to do with uncommitted changes when switching to another branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Changes {
  /// Stash them on the current branch; they wait there and can be restored when coming back.
  Leave,
  /// Take them along to the other branch.
  Bring,
}

fn message(error: git2::Error) -> String {
  error.message().to_string()
}

fn git(path: &Path, arguments: &[&str]) -> Result<Output, String> {
  ws_activity::timed(
    "git",
    || crate::describe_git(path, arguments),
    || {
      let mut command = Command::new("git");
      crate::shell_path::apply(&mut command);
      command
        .arg("-C")
        .arg(path)
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not run git: {error}"))
    },
    crate::describe_output,
  )
}

fn checked(output: Output) -> Result<String, String> {
  if output.status.success() {
    return Ok(String::from_utf8_lossy(&output.stdout).to_string());
  }
  let stderr = String::from_utf8_lossy(&output.stderr);
  let stdout = String::from_utf8_lossy(&output.stdout);
  let text = if stderr.trim().is_empty() { stdout } else { stderr };
  Err(if text.trim().is_empty() { format!("git exited with {}.", output.status) } else { text.trim().to_string() })
}

/// The local branches, and the branches only a remote has, sorted by name. A remote branch a local one stands for is not listed twice.
pub fn branch_entries(path: &Path) -> Result<Vec<BranchEntry>, String> {
  let repository = Repository::discover(path).map_err(message)?;
  let mut locals: Vec<(String, Option<i64>)> = Vec::new();
  let mut remotes: Vec<(String, String, Option<i64>)> = Vec::new();
  for (branch, kind) in repository.branches(None).map_err(message)?.filter_map(Result::ok) {
    let Some(name) = branch.name().ok().flatten().map(str::to_string) else { continue };
    let time = branch.get().peel_to_commit().ok().map(|commit| commit.time().seconds());
    if kind == BranchType::Local {
      locals.push((name, time));
    } else if let Some((remote, short)) = name.split_once('/').filter(|(_, short)| *short != "HEAD") {
      remotes.push((remote.to_string(), short.to_string(), time));
    }
  }
  let mut seen: HashSet<String> = locals.iter().map(|(name, _)| name.clone()).collect();
  let mut entries: Vec<BranchEntry> = locals.into_iter().map(|(name, time)| BranchEntry { name, remote: None, time, default: false }).collect();
  remotes.sort_by_key(|(remote, _, _)| (remote != "origin", remote.clone()));
  for (remote, short, time) in remotes {
    if seen.insert(short.clone()) {
      entries.push(BranchEntry { name: short, remote: Some(remote), time, default: false });
    }
  }
  entries.sort_by_key(|entry| entry.name.to_lowercase());
  if let Some(name) = default_branch_name(&repository, &entries) {
    if let Some(entry) = entries.iter_mut().find(|entry| entry.name == name) {
      entry.default = true;
    }
  }
  Ok(entries)
}

/// The name of the default branch: where `origin/HEAD` points, else the first of `main`, `master`, `develop` the list has.
fn default_branch_name(repository: &Repository, entries: &[BranchEntry]) -> Option<String> {
  let pointed = repository.remotes().ok()?.iter().flatten().flatten().find_map(|remote| {
    let reference = repository.find_reference(&format!("refs/remotes/{remote}/HEAD")).ok()?;
    let target = reference.symbolic_target().ok()??;
    target.strip_prefix(&format!("refs/remotes/{remote}/")).map(str::to_string)
  });
  pointed
    .filter(|name| entries.iter().any(|entry| &entry.name == name))
    .or_else(|| ["main", "master", "develop"].into_iter().find(|name| entries.iter().any(|entry| entry.name == *name)).map(str::to_string))
}

/// Makes a local branch that follows a remote one and switches to it, like `git switch <name>` does when only a remote has it.
pub(crate) fn track_remote_branch(repository: &Repository, name: &str) -> Result<(), String> {
  let remote_name = repository
    .remotes()
    .map_err(message)?
    .iter()
    .flatten()
    .flatten()
    .map(str::to_string)
    .filter(|remote| repository.find_branch(&format!("{remote}/{name}"), BranchType::Remote).is_ok())
    .min_by_key(|remote| remote != "origin")
    .ok_or_else(|| format!("There is no branch named {name}."))?;
  let remote_branch = repository.find_branch(&format!("{remote_name}/{name}"), BranchType::Remote).map_err(message)?;
  let commit = remote_branch.get().peel_to_commit().map_err(message)?;
  let mut local = repository.branch(name, &commit, false).map_err(message)?;
  local.set_upstream(Some(&format!("{remote_name}/{name}"))).map_err(message)
}

/// The remote a local branch follows, when its upstream branch still exists.
pub fn upstream_remote(path: &Path, name: &str) -> Option<String> {
  let repository = Repository::discover(path).ok()?;
  let branch = repository.find_branch(name, BranchType::Local).ok()?;
  branch.upstream().ok()?;
  let reference = branch.get().name().ok()?.to_string();
  repository.branch_upstream_remote(&reference).ok()?.as_str().ok().map(str::to_string)
}

/// Deletes a local branch (its commits stay in the repository until git cleans up). The branch checked out cannot be deleted.
pub fn delete_branch(path: &Path, name: &str) -> Result<(), String> {
  let repository = Repository::discover(path).map_err(message)?;
  let mut branch = repository.find_branch(name, BranchType::Local).map_err(|_| format!("There is no local branch named {name}."))?;
  if branch.is_head() {
    return Err("The branch you are on cannot be deleted. Switch to another branch first.".into());
  }
  branch.delete().map_err(message)
}

/// Deletes the branch on the remote that has it (`git push <remote> --delete`).
pub fn delete_remote_branch(
  path: &Path,
  remote: &str,
  name: &str,
  credentials: Option<&Credentials>,
  mut on_progress: impl FnMut(&str),
) -> Result<String, String> {
  if name.starts_with('-') || remote.starts_with('-') {
    return Err(format!("\"{name}\" is not a branch."));
  }
  super::with_login(credentials, |credentials| {
    super::finish(
      super::run_git(path, &["push", remote, "--delete", name], credentials, &mut on_progress)?,
      "Could not delete the branch on the remote.",
    )
  })
}

/// Renames a local branch. The name must be valid and not taken; a branch that follows a remote keeps following it.
pub fn rename_branch(path: &Path, old: &str, new: &str) -> Result<(), String> {
  let new = new.trim();
  if new.is_empty() {
    return Err("Enter a branch name.".into());
  }
  if !git2::Branch::name_is_valid(new).unwrap_or(false) {
    return Err(format!("\"{new}\" is not a valid branch name."));
  }
  let repository = Repository::discover(path).map_err(message)?;
  if repository.find_branch(new, BranchType::Local).is_ok() {
    return Err(format!("A branch named {new} already exists."));
  }
  let mut branch = repository.find_branch(old, BranchType::Local).map_err(|_| format!("There is no local branch named {old}."))?;
  branch.rename(new, false).map(|_| ()).map_err(message)
}

/// Whether the working tree has changes, untracked files included.
pub fn has_changes(path: &Path) -> bool {
  super::quick_status(path).is_some_and(|(changed, _)| changed > 0)
}

/// Switches to a branch with the changes in the working tree set aside as asked. [`Changes::Leave`] stashes them under the
/// branch being left; [`Changes::Bring`] stashes them, switches, and applies them on the other branch (a conflict leaves them in
/// the stash and says so, so nothing is lost).
pub fn switch_branch_with(path: &Path, name: &str, changes: Changes) -> Result<(), String> {
  let repository = Repository::discover(path).map_err(message)?;
  if repository.state() == RepositoryState::Merge {
    return Err("Finish or abort the current merge first.".into());
  }
  let current = repository.head().ok().and_then(|head| head.shorthand().ok().map(str::to_string)).unwrap_or_default();
  if !has_changes(path) {
    return super::switch_branch(path, name);
  }
  if !branch_entries(path)?.iter().any(|entry| entry.name == name) {
    return Err(format!("There is no branch named {name}."));
  }
  stash_changes(path, &current)?;
  if let Err(error) = super::switch_branch(path, name) {
    // Put everything back: a switch that failed must not leave the changes hidden.
    let _ = restore_stash(path, &current);
    return Err(error);
  }
  if changes == Changes::Bring {
    if let Err(error) = restore_stash(path, &current) {
      return Err(format!("Switched to {name}, but your changes could not be applied and stay stashed on {current}: {error}"));
    }
  }
  Ok(())
}

/// Creates a branch and switches to it with the changes in the working tree set aside as asked. [`Changes::Bring`] is what
/// [`super::create_branch`] does; [`Changes::Leave`] stashes them under the branch being left first, so the new branch starts
/// clean, and puts them back if the branch could not be made.
pub fn create_branch_with(path: &Path, name: &str, changes: Changes) -> Result<(), String> {
  let name = name.trim();
  let repository = super::check_new_branch(path, name)?;
  let current = repository.head().ok().and_then(|head| head.shorthand().ok().map(str::to_string)).unwrap_or_default();
  if changes == Changes::Bring || !has_changes(path) {
    return super::create_branch(path, name);
  }
  stash_changes(path, &current)?;
  if let Err(error) = super::create_branch(path, name) {
    let _ = restore_stash(path, &current);
    return Err(error);
  }
  Ok(())
}

fn stash_message(branch: &str) -> String {
  format!("{STASH_MARK}{branch}>")
}

/// Stashes every change, untracked files included, as the changes of `branch`. Replaces an earlier stash of the same branch only
/// once the new one exists, so nothing is lost if it fails.
pub fn stash_changes(path: &Path, branch: &str) -> Result<(), String> {
  let previous = stash_index(path, branch)?;
  checked(git(path, &["stash", "push", "--include-untracked", "-m", &stash_message(branch)])?)?;
  if let Some(index) = previous {
    // The older stash moved one place down when the new one went on top.
    checked(git(path, &["stash", "drop", &format!("stash@{{{}}}", index + 1)])?)?;
  }
  Ok(())
}

fn stash_index(path: &Path, branch: &str) -> Result<Option<usize>, String> {
  let listing = checked(git(path, &["stash", "list", "--format=%gd%x09%s"])?)?;
  let wanted = stash_message(branch);
  Ok(
    listing.lines().filter_map(|line| line.split_once('\t')).enumerate().find(|(_, (_, subject))| subject.ends_with(&wanted)).map(|(index, _)| index),
  )
}

/// Whether changes were stashed on this branch by [`stash_changes`].
pub fn has_stash(path: &Path, branch: &str) -> bool {
  stash_index(path, branch).ok().flatten().is_some()
}

/// Applies the changes stashed on this branch and drops the stash. When applying fails (a conflict with what is there now) the
/// stash stays.
pub fn restore_stash(path: &Path, branch: &str) -> Result<(), String> {
  let index = stash_index(path, branch)?.ok_or("There are no stashed changes on this branch.")?;
  checked(git(path, &["stash", "pop", &format!("stash@{{{index}}}")])?).map(|_| ())
}

/// Throws away the changes stashed on this branch.
pub fn discard_stash(path: &Path, branch: &str) -> Result<(), String> {
  let index = stash_index(path, branch)?.ok_or("There are no stashed changes on this branch.")?;
  checked(git(path, &["stash", "drop", &format!("stash@{{{index}}}")])?).map(|_| ())
}
