//! Comparing the current branch with another one, and merging that one into it.

use super::{commit_info, CommitInfo};
use git2::{BranchType, Repository};
use std::{
  collections::{HashMap, HashSet},
  path::Path,
};

/// How many commits of each side of a comparison are listed.
pub const COMPARE_LIMIT: usize = 300;

/// How many recently used branches the list offers.
const RECENT_LIMIT: usize = 5;

/// A branch to compare with, and how it stands against the current branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchRef {
  /// A local branch (`develop`) or a remote one (`origin/develop`).
  pub name: String,
  /// Commits the current branch has that this one does not.
  pub ahead: usize,
  /// Commits this branch has that the current one does not.
  pub behind: usize,
}

/// The branches to choose from, in the groups the list shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BranchChoices {
  /// The branch the remote calls its default (else `main` or `master`).
  pub default: Option<BranchRef>,
  /// The branches checked out lately.
  pub recent: Vec<BranchRef>,
  /// All the others, local and remote, by name.
  pub other: Vec<BranchRef>,
}

/// The two sides of a comparison.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comparison {
  /// Commits of the other branch the current one does not have, newest first (at most [`COMPARE_LIMIT`]).
  pub behind: Vec<CommitInfo>,
  /// Commits of the current branch the other one does not have, newest first (at most [`COMPARE_LIMIT`]).
  pub ahead: Vec<CommitInfo>,
  /// The real numbers, which can be larger than the lists.
  pub behind_count: usize,
  pub ahead_count: usize,
  /// Number of files predicted to conflict before the working tree is changed.
  pub conflict_count: Option<usize>,
}

fn message(error: git2::Error) -> String {
  error.message().to_string()
}

/// Every branch except the current one, with where it stands against the current branch.
pub fn compare_choices(path: &Path) -> Result<BranchChoices, String> {
  let repository = Repository::discover(path).map_err(message)?;
  let head = repository.head().map_err(message)?;
  let head_id = head.peel_to_commit().map_err(message)?.id();
  let current = head.shorthand().map(str::to_string);
  let mut locals = HashSet::new();
  let mut found: Vec<(String, git2::Oid)> = Vec::new();
  for (branch, kind) in repository.branches(None).map_err(message)?.filter_map(Result::ok) {
    let Some(name) = branch.name().ok().flatten().map(str::to_string) else { continue };
    let Ok(commit) = branch.get().peel_to_commit() else { continue };
    if kind == BranchType::Local {
      locals.insert(name.clone());
    } else if name.ends_with("/HEAD") {
      continue;
    }
    found.push((name, commit.id()));
  }
  // A remote branch that a local one of the same name stands for is not listed twice.
  found.retain(|(name, _)| {
    let remote_copy = name.split_once('/').is_some_and(|(_, short)| locals.contains(short)) && !locals.contains(name);
    !remote_copy && current.as_deref() != Some(name.as_str())
  });
  let counted: HashMap<String, BranchRef> = found
    .into_iter()
    .filter_map(|(name, id)| {
      let (ahead, behind) = repository.graph_ahead_behind(head_id, id).ok()?;
      Some((name.clone(), BranchRef { name, ahead, behind }))
    })
    .collect();
  let mut taken: HashSet<String> = HashSet::new();
  let mut take = |name: &str| -> Option<BranchRef> {
    let branch = counted.get(name)?;
    taken.insert(name.to_string()).then(|| branch.clone())
  };
  let default_name = repository
    .find_reference("refs/remotes/origin/HEAD")
    .ok()
    .and_then(|reference| reference.symbolic_target().and_then(|target| target.strip_prefix("refs/remotes/origin/").map(str::to_string)))
    .map(|short| if locals.contains(&short) { short } else { format!("origin/{short}") })
    .or_else(|| ["main", "master"].iter().find(|name| locals.contains(**name)).map(|name| (*name).to_string()));
  let default = default_name.and_then(|name| take(&name));
  let mut recent = Vec::new();
  if let Ok(reflog) = repository.reflog("HEAD") {
    'entries: for entry in reflog.iter() {
      let moved = entry.message().and_then(|text| text.strip_prefix("checkout: moving from ")).and_then(|rest| rest.split_once(" to "));
      for name in moved.into_iter().flat_map(|(from, to)| [to, from]) {
        if locals.contains(name) {
          if let Some(branch) = take(name) {
            recent.push(branch);
          }
        }
        if recent.len() >= RECENT_LIMIT {
          break 'entries;
        }
      }
    }
  }
  let mut other: Vec<BranchRef> = counted.into_iter().filter(|(name, _)| !taken.contains(name)).map(|(_, branch)| branch).collect();
  other.sort_by_key(|branch| branch.name.to_lowercase());
  Ok(BranchChoices { default, recent, other })
}

/// The commits that differ between the current branch and `branch`, on each side.
pub fn compare(path: &Path, branch: &str) -> Result<Comparison, String> {
  let repository = Repository::discover(path).map_err(message)?;
  let head_id = repository.head().map_err(message)?.peel_to_commit().map_err(message)?.id();
  let other_id =
    repository.revparse_single(branch).map_err(|_| format!("There is no branch named {branch}."))?.peel_to_commit().map_err(message)?.id();
  let (ahead_count, behind_count) = repository.graph_ahead_behind(head_id, other_id).map_err(message)?;
  let side = |from: git2::Oid, without: git2::Oid| -> Result<Vec<CommitInfo>, String> {
    let mut walk = repository.revwalk().map_err(message)?;
    walk.set_sorting(git2::Sort::TIME).map_err(message)?;
    walk.push(from).map_err(message)?;
    walk.hide(without).map_err(message)?;
    let (unpushed, tags) = (HashSet::new(), HashMap::new());
    Ok(
      walk
        .filter_map(Result::ok)
        .filter_map(|id| repository.find_commit(id).ok())
        .take(COMPARE_LIMIT)
        .map(|commit| commit_info(&commit, &unpushed, &tags))
        .collect(),
    )
  };
  Ok(Comparison {
    behind: side(other_id, head_id)?,
    ahead: side(head_id, other_id)?,
    behind_count,
    ahead_count,
    conflict_count: super::merge::preview(&repository, head_id, other_id).ok(),
  })
}

/// Merges `branch` into the current branch, like `git merge --no-edit`. With conflicts or changes in the way nothing is
/// left half done: the merge is stopped and the files are as they were.
pub fn merge_branch(path: &Path, branch: &str) -> Result<(), String> {
  if branch.starts_with('-') || branch.trim().is_empty() {
    return Err(format!("\"{branch}\" is not a branch."));
  }
  let git = |arguments: &[&str]| {
    std::process::Command::new("git")
      .arg("-C")
      .arg(path)
      .args(arguments)
      .env("GIT_TERMINAL_PROMPT", "0")
      .env("GIT_EDITOR", "true")
      .stdin(std::process::Stdio::null())
      .output()
      .map_err(|error| format!("Could not run git: {error}"))
  };
  let output = git(&["merge", "--no-edit", branch])?;
  if output.status.success() {
    return Ok(());
  }
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  let conflicts: Vec<&str> = stdout.lines().filter_map(|line| line.split_once("Merge conflict in ").map(|(_, file)| file.trim())).collect();
  // Stops a merge that was left waiting for the conflicts to be solved; harmless when none was started.
  let _ = git(&["merge", "--abort"]);
  if conflicts.is_empty() {
    let reason = stderr.lines().chain(stdout.lines()).rev().find(|line| !line.trim().is_empty()).unwrap_or("git could not merge.").trim().to_string();
    Err(reason)
  } else {
    Err(format!("Merging {branch} conflicts in {}. Nothing was changed.", conflicts.join(", ")))
  }
}
