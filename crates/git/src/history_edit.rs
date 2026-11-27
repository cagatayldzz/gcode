//! Explicit history rewrites in one working tree.

use crate::commit_actions::{failure, git};
use git2::{Oid, Repository};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryHead {
  reference: String,
  id: Oid,
}

impl HistoryHead {
  pub fn label(&self) -> String {
    self.reference.strip_prefix("refs/heads/").map(str::to_owned).unwrap_or_else(|| format!("detached HEAD at {:.7}", self.id.to_string()))
  }
}

fn read_head(repository: &Repository) -> Result<HistoryHead, String> {
  let head = repository.head().map_err(|error| error.to_string())?;
  let id = head.peel_to_commit().map_err(|error| error.to_string())?.id();
  let reference = if head.is_branch() { head.name().map_err(|error| error.to_string())?.to_owned() } else { "HEAD".into() };
  Ok(HistoryHead { reference, id })
}

pub fn history_head(path: &Path) -> Result<HistoryHead, String> {
  let repository = Repository::discover(path).map_err(|error| error.to_string())?;
  read_head(&repository)
}

fn checked_repository(path: &Path, expected: &HistoryHead) -> Result<Repository, String> {
  let repository = Repository::discover(path).map_err(|error| error.to_string())?;
  crate::operation::require_idle(&repository)?;
  if read_head(&repository)? != *expected {
    return Err("HEAD changed after you opened the confirmation. Review the new history before trying again.".into());
  }
  Ok(repository)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetMode {
  Soft,
  Mixed,
  Hard,
}

pub fn reset_to_commit(path: &Path, expected: &HistoryHead, id: &str, mode: ResetMode) -> Result<(), String> {
  let repository = checked_repository(path, expected)?;
  let id = Oid::from_str(id).map_err(|error| error.to_string())?;
  let commit = repository.find_commit(id).map_err(|error| error.to_string())?;
  let option = match mode {
    ResetMode::Soft => "--soft",
    ResetMode::Mixed => "--mixed",
    ResetMode::Hard => "--hard",
  };
  let output = git(path, &["reset", option, &commit.id().to_string(), "--"])?;
  if !output.status.success() {
    return Err(failure(&output, "Git could not reset to the commit."));
  }
  Ok(())
}

pub fn rebase_onto(path: &Path, expected: &HistoryHead, branch: &str, remote: Option<&str>) -> Result<crate::Operation, String> {
  let repository = checked_repository(path, expected)?;
  if !expected.reference.starts_with("refs/heads/") {
    return Err("Switch to a local branch before starting a rebase.".into());
  }
  let base = match remote {
    Some(remote) => format!("refs/remotes/{remote}/{branch}"),
    None => format!("refs/heads/{branch}"),
  };
  if base == expected.reference {
    return Err("Choose another branch as the rebase base.".into());
  }
  let upstream = repository.find_reference(&base).and_then(|reference| reference.peel_to_commit()).map_err(|error| error.to_string())?;
  // Native Git checks tracked/index changes and refuses to overwrite obstructing untracked files.
  let output = git(path, &["rebase", "--no-autostash", "--no-update-refs", "--rebase-merges", "--", &upstream.id().to_string()])?;
  let operation = crate::operation(path)?;
  if !output.status.success() && operation != crate::Operation::Rebase {
    return Err(failure(&output, "Git could not rebase the branch."));
  }
  Ok(operation)
}
