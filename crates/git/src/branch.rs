//! Branches: remote branches next to the local ones, deleting and renaming, and what happens to
//! uncommitted changes when switching.

use super::Credentials;
use git2::{BranchType, Repository};
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
  super::worktrees::require_branch_available(&repository, name)?;
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
  super::worktrees::require_branch_available(&repository, old)?;
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
  super::operation::require_idle(&repository)?;
  let current = repository.head().ok().and_then(|head| head.shorthand().ok().map(str::to_string)).unwrap_or_default();
  if !has_changes(path) {
    return super::switch_branch(path, name);
  }
  if !branch_entries(path)?.iter().any(|entry| entry.name == name) {
    return Err(format!("There is no branch named {name}."));
  }
  let stashed = push_stash(path, &current)?;
  if let Err(error) = super::switch_branch(path, name) {
    return Err(restore_after_failure(path, &current, stashed.as_deref(), error));
  }
  if let (Changes::Bring, Some(id)) = (changes, stashed) {
    restore_stash_by_id(path, &current, &id).map_err(|error| format!("Switched to {name}. {error}"))?;
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
  let stashed = push_stash(path, &current)?;
  if let Err(error) = super::create_branch(path, name) {
    return Err(restore_after_failure(path, &current, stashed.as_deref(), error));
  }
  Ok(())
}

fn stash_message(branch: &str) -> String {
  format!("{STASH_MARK}{branch}>")
}

/// Adds a new branch stash, including untracked files, and retains previous entries.
pub fn stash_changes(path: &Path, branch: &str) -> Result<(), String> {
  push_stash(path, branch).map(|_| ())
}

fn push_stash(path: &Path, branch: &str) -> Result<Option<String>, String> {
  use std::sync::atomic::{AtomicU64, Ordering};
  use std::time::SystemTime;

  static NEXT: AtomicU64 = AtomicU64::new(0);
  let repository = Repository::discover(path).map_err(message)?;
  super::operation::require_idle(&repository)?;
  let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_err(|error| error.to_string())?.as_nanos();
  let label = format!("{}-{time}-{} {}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed), stash_message(branch));
  checked(git(path, &["stash", "push", "--include-untracked", "-m", &label])?)?;
  let suffix = format!(": {label}");
  Ok(stash_list(path)?.into_iter().find(|entry| entry.subject.ends_with(&suffix)).map(|entry| entry.id))
}

struct Stash {
  id: String,
  selector: String,
  subject: String,
}

fn stash_list(path: &Path) -> Result<Vec<Stash>, String> {
  let listing = checked(git(path, &["stash", "list", "--format=%H%x09%gd%x09%s"])?)?;
  Ok(
    listing
      .lines()
      .filter_map(|line| {
        let mut fields = line.splitn(3, '\t');
        Some(Stash { id: fields.next()?.into(), selector: fields.next()?.into(), subject: fields.next()?.into() })
      })
      .collect(),
  )
}

fn branch_stashes(path: &Path, branch: &str) -> Result<Vec<Stash>, String> {
  let wanted = stash_message(branch);
  Ok(stash_list(path)?.into_iter().filter(|entry| entry.subject.ends_with(&wanted)).collect())
}

/// Stable commit IDs of this branch's app stashes, newest first.
pub fn stash_ids(path: &Path, branch: &str) -> Result<Vec<String>, String> {
  Ok(branch_stashes(path, branch)?.into_iter().map(|entry| entry.id).collect())
}

pub fn has_stash(path: &Path, branch: &str) -> bool {
  stash_ids(path, branch).is_ok_and(|ids| !ids.is_empty())
}

fn latest_stash(path: &Path, branch: &str) -> Result<String, String> {
  stash_ids(path, branch)?.into_iter().next().ok_or("There are no stashed changes on this branch.".into())
}

pub fn restore_stash(path: &Path, branch: &str) -> Result<(), String> {
  restore_stash_by_id(path, branch, &latest_stash(path, branch)?)
}

pub fn discard_stash(path: &Path, branch: &str) -> Result<(), String> {
  discard_stash_by_id(path, branch, &latest_stash(path, branch)?)
}

pub fn restore_stash_by_id(path: &Path, branch: &str, id: &str) -> Result<(), String> {
  find_stash(path, branch, id)?;
  let repository = Repository::discover(path).map_err(message)?;
  super::operation::require_idle(&repository)?;
  checked(git(path, &["stash", "apply", "--index", id])?)
    .map_err(|error| format!("Your changes stay stashed on {branch}. Review the working tree before retrying. {error}"))?;
  discard_stash_by_id(path, branch, id)
    .map_err(|error| format!("Applied the changes to the working tree, but could not remove the stash entry. {error}"))
}

pub fn discard_stash_by_id(path: &Path, branch: &str, id: &str) -> Result<(), String> {
  let entry = find_stash(path, branch, id)?;
  // Resolve the current selector from the object ID, since a terminal command may have moved the stash stack.
  checked(git(path, &["stash", "drop", &entry.selector])?).map(|_| ())
}

fn find_stash(path: &Path, branch: &str, id: &str) -> Result<Stash, String> {
  branch_stashes(path, branch)?
    .into_iter()
    .find(|entry| entry.id == id)
    .ok_or("That stash is no longer on this branch. Refresh the stash list.".into())
}

fn restore_after_failure(path: &Path, branch: &str, stashed: Option<&str>, error: String) -> String {
  if let Some(id) = stashed {
    let on_source = Repository::discover(path)
      .ok()
      .and_then(|repository| repository.head().ok()?.shorthand().ok().map(str::to_owned))
      .is_some_and(|head| head == branch);
    if !on_source || !super::quick_status(path).is_some_and(|(changed, _)| changed == 0) {
      return format!("{error} Your changes stay stashed on {branch}. Review the working tree before restoring them.");
    }
    if let Err(restore) = restore_stash_by_id(path, branch, id) {
      return format!("{error} Could not restore the working tree automatically. {restore}");
    }
  }
  error
}
