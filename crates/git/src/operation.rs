use git2::{Repository, RepositoryState};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
  Idle,
  Merge,
  Rebase,
  CherryPick,
  Revert,
  Bisect,
  ApplyMailbox,
}

impl Operation {
  pub(crate) fn read(repository: &Repository) -> Self {
    match repository.state() {
      RepositoryState::Clean => Self::Idle,
      RepositoryState::Merge => Self::Merge,
      RepositoryState::Rebase | RepositoryState::RebaseInteractive | RepositoryState::RebaseMerge => Self::Rebase,
      RepositoryState::CherryPick | RepositoryState::CherryPickSequence => Self::CherryPick,
      RepositoryState::Revert | RepositoryState::RevertSequence => Self::Revert,
      RepositoryState::Bisect => Self::Bisect,
      RepositoryState::ApplyMailbox | RepositoryState::ApplyMailboxOrRebase => Self::ApplyMailbox,
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::Idle => "No Git operation",
      Self::Merge => "Merge",
      Self::Rebase => "Rebase",
      Self::CherryPick => "Cherry-pick",
      Self::Revert => "Revert",
      Self::Bisect => "Bisect",
      Self::ApplyMailbox => "Patch application",
    }
  }

  fn command(self) -> Option<&'static str> {
    match self {
      Self::Rebase => Some("rebase"),
      Self::CherryPick => Some("cherry-pick"),
      Self::Revert => Some("revert"),
      _ => None,
    }
  }

  pub fn can_resume(self) -> bool {
    self.command().is_some()
  }

  fn blocked(self) -> String {
    format!("{} in progress. Finish or abort it before starting another Git operation.", self.label())
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationStep {
  Continue,
  Skip,
  Abort,
}

pub fn operation(path: &Path) -> Result<Operation, String> {
  Repository::discover(path).map(|repository| Operation::read(&repository)).map_err(|error| error.message().to_string())
}

pub(crate) fn require_idle(repository: &Repository) -> Result<(), String> {
  let operation = Operation::read(repository);
  if operation == Operation::Idle {
    Ok(())
  } else {
    Err(operation.blocked())
  }
}

pub fn resume_operation(path: &Path, expected: Operation, step: OperationStep) -> Result<Operation, String> {
  if operation(path)? != expected {
    return Err("The Git operation changed. Refresh before trying again.".into());
  }
  let command = expected.command().ok_or("Finish this Git operation in the terminal.")?;
  let argument = match step {
    OperationStep::Continue => "--continue",
    OperationStep::Skip => "--skip",
    OperationStep::Abort => "--abort",
  };
  let output = super::commit_actions::git(path, &[command, argument])?;
  if !output.status.success() {
    let diagnostics = super::commit_actions::failure(&output, "Git could not resume the operation.");
    return Err(format!("{} {argument}: {diagnostics}", expected.label()));
  }
  operation(path)
}
