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
