//! Starts, resolves, and finishes a merge in the repository's working tree.

use git2::{IndexEntry, Repository, RepositoryState};
use std::{
  path::Path,
  process::{Command, Output, Stdio},
};

#[derive(Clone, Debug)]
pub struct MergeFile {
  pub path: String,
  pub conflicts: usize,
  pub ours: bool,
  pub theirs: bool,
  pub can_choose_side: bool,
}

#[derive(Clone, Debug)]
pub struct MergeState {
  head: git2::Oid,
  incoming: String,
  pub current_branch: String,
  pub files: Vec<MergeFile>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeSide {
  Current,
  Incoming,
}

#[derive(Debug)]
pub enum MergeOutcome {
  Finished,
  Conflicted(MergeState),
}

fn error(error: git2::Error) -> String {
  error.message().into()
}

fn git(path: &Path, args: &[&str]) -> Result<Output, String> {
  Command::new("git")
    .arg("-C")
    .arg(path)
    .args(args)
    .env("GIT_TERMINAL_PROMPT", "0")
    .env("GIT_EDITOR", "true")
    .env("GIT_LITERAL_PATHSPECS", "1")
    .stdin(Stdio::null())
    .output()
    .map_err(|error| format!("Could not run Git: {error}"))
}

fn checked(output: Output) -> Result<(), String> {
  if output.status.success() {
    return Ok(());
  }
  let stderr = String::from_utf8_lossy(&output.stderr);
  let stdout = String::from_utf8_lossy(&output.stdout);
  let message = stderr.trim();
  Err(if !message.is_empty() {
    message.to_string()
  } else if !stdout.trim().is_empty() {
    stdout.trim().to_string()
  } else {
    format!("Git exited with {}.", output.status)
  })
}

fn merge_files(repository: &Repository, index: &git2::Index) -> Result<Vec<MergeFile>, String> {
  let mut files = std::collections::BTreeMap::new();
  for conflict in index.conflicts().map_err(error)? {
    let conflict = conflict.map_err(error)?;
    let entries = [&conflict.ancestor, &conflict.our, &conflict.their];
    let mut paths: Vec<_> = entries.iter().filter_map(|entry| entry.as_ref().map(|entry| entry.path.as_slice())).collect();
    paths.sort_unstable();
    paths.dedup();
    let same_path = paths.len() == 1;
    let at_path = |entry: &Option<IndexEntry>, path: &[u8]| entry.as_ref().is_some_and(|entry| entry.path == path);
    for path in paths {
      let supported_path = std::str::from_utf8(path).is_ok();
      let name = String::from_utf8_lossy(path).into_owned();
      let conflicts = repository
        .workdir()
        .and_then(|root| {
          let file = root.join(&name);
          let metadata = file.symlink_metadata().ok()?;
          (metadata.is_file() && metadata.len() <= 1024 * 1024).then(|| std::fs::read(file).ok()).flatten()
        })
        .map_or(1, |text| text.split(|byte| *byte == b'\n').filter(|line| line.starts_with(b"<<<<<<< ")).count().max(1));
      files.entry(name.clone()).or_insert_with(|| MergeFile {
        path: name,
        conflicts,
        ours: at_path(&conflict.our, path),
        theirs: at_path(&conflict.their, path),
        can_choose_side: supported_path && same_path && entries.into_iter().filter_map(Option::as_ref).all(|entry| entry.mode != 0o160000),
      });
    }
  }
  Ok(files.into_values().collect())
}

pub(crate) fn preview(repository: &Repository, head: git2::Oid, other: git2::Oid) -> Result<usize, String> {
  let head = repository.find_commit(head).map_err(error)?;
  let other = repository.find_commit(other).map_err(error)?;
  let index = repository.merge_commits(&head, &other, None).map_err(error)?;
  let mut paths = std::collections::HashSet::new();
  for conflict in index.conflicts().map_err(error)? {
    let conflict = conflict.map_err(error)?;
    paths.extend([conflict.ancestor, conflict.our, conflict.their].into_iter().flatten().map(|entry| entry.path));
  }
  Ok(paths.len())
}

pub fn read_merge(path: &Path) -> Result<MergeState, String> {
  let repository = Repository::discover(path).map_err(error)?;
  if repository.state() != RepositoryState::Merge {
    return Err("There is no merge in progress.".into());
  }
  let head = repository.head().map_err(error)?;
  let incoming = std::fs::read_to_string(repository.path().join("MERGE_HEAD")).map_err(|error| error.to_string())?;
  let files = merge_files(&repository, &repository.index().map_err(error)?)?;
  let current_branch = head.shorthand().unwrap_or("HEAD").into();
  let head = head.peel_to_commit().map_err(error)?.id();
  Ok(MergeState { head, incoming, current_branch, files })
}

pub fn start_merge(path: &Path, branch: &str, current_branch: &str) -> Result<MergeOutcome, String> {
  if branch.starts_with('-') || branch.trim().is_empty() {
    return Err("Select a branch to merge.".into());
  }
  let repository = Repository::discover(path).map_err(error)?;
  if repository.state() != RepositoryState::Clean || repository.head().map_err(error)?.shorthand().ok() != Some(current_branch) {
    return Err("The repository or current branch changed. Refresh before merging.".into());
  }
  // Uncommitted changes are fine: git itself refuses, and changes nothing, when the merge would overwrite them.
  let output = git(path, &["merge", "--no-ff", "--no-edit", "--", branch])?;
  if output.status.success() {
    return Ok(MergeOutcome::Finished);
  }
  if repository.state() == RepositoryState::Merge {
    return read_merge(path).map(MergeOutcome::Conflicted);
  }
  checked(output)?;
  Ok(MergeOutcome::Finished)
}

impl MergeState {
  pub fn same_merge(&self, other: &Self) -> bool {
    (self.head, &self.incoming, &self.current_branch) == (other.head, &other.incoming, &other.current_branch)
  }

  pub fn refresh(&self, path: &Path) -> Result<Self, String> {
    let current = read_merge(path)?;
    if !self.same_merge(&current) {
      return Err("The merge in this repository changed. Reopen the conflict resolver.".into());
    }
    Ok(current)
  }

  pub fn resolve(&self, path: &Path, file: &str, side: MergeSide) -> Result<Self, String> {
    let current = self.refresh(path)?;
    let item = current
      .files
      .iter()
      .find(|item| item.path == file && item.can_choose_side)
      .ok_or("This file no longer has a conflict that can be resolved by choosing a side.")?;
    let exists = if side == MergeSide::Current { item.ours } else { item.theirs };
    let workdir = super::workdir_file(path, Path::new("")).ok_or("Could not find the repository working directory.")?;
    if exists {
      let flag = if side == MergeSide::Current { "--ours" } else { "--theirs" };
      checked(git(&workdir, &["checkout", flag, "--", file])?)?;
      checked(git(&workdir, &["add", "--", file])?)?;
    } else {
      checked(git(&workdir, &["rm", "-f", "--", file])?)?;
    }
    self.refresh(path)
  }

  pub fn undo(&self, path: &Path, file: &str) -> Result<Self, String> {
    self.refresh(path)?;
    let workdir = super::workdir_file(path, Path::new("")).ok_or("Could not find the repository working directory.")?;
    let undo = git(&workdir, &["ls-files", "--resolve-undo", "--", file])?;
    if undo.stdout.is_empty() {
      return Err("Git no longer has a conflict resolution to undo for this file.".into());
    }
    if !git(&workdir, &["diff", "--quiet", "--", file])?.status.success() {
      return Err("Save or set aside unstaged edits to this file before undoing its resolution.".into());
    }
    checked(git(&workdir, &["checkout", "-m", "--", file])?)?;
    self.refresh(path)
  }

  pub fn finish(&self, path: &Path, abort: bool) -> Result<(), String> {
    let current = self.refresh(path)?;
    if !abort && !current.files.is_empty() {
      return Err("Resolve and stage every conflicted file before continuing.".into());
    }
    checked(git(path, &["merge", if abort { "--abort" } else { "--continue" }])?)
  }
}
