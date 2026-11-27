//! Diffs between Git's three versions of a file and updates to the index alone.

use crate::commit::{literal, run, validate_paths, Head, IndexTransaction};
use git2::Repository;
use std::{
  ffi::{OsStr, OsString},
  fs,
  ops::Range,
  path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageSide {
  Unstaged,
  Staged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageDiff {
  patch: Vec<u8>,
  ranges: Vec<Range<usize>>,
  untracked: bool,
}

impl StageDiff {
  pub fn text(&self) -> String {
    String::from_utf8_lossy(&self.patch).into_owned()
  }

  pub fn is_untracked(&self) -> bool {
    self.untracked
  }

  pub fn has_changes(&self) -> bool {
    self.untracked || !self.patch.is_empty()
  }

  pub fn hunks(&self) -> impl Iterator<Item = &str> {
    self.ranges.iter().filter_map(|range| std::str::from_utf8(&self.patch[range.clone()]).ok()?.lines().next())
  }

  fn new(patch: Vec<u8>, single_file: bool) -> Self {
    let mut starts = Vec::new();
    let mut position = 0;
    let mut files_seen = 0;
    if single_file {
      for line in patch.split_inclusive(|byte| *byte == b'\n') {
        if line.starts_with(b"diff --git ") {
          files_seen += 1;
        }
        if line.starts_with(b"@@ ") {
          starts.push(position);
        }
        position += line.len();
      }
    }
    // File creation, deletion, binary data and mode changes use the whole-file action.
    let ranges = if let Some(first) = starts.first().copied().filter(|_| files_seen == 1 && std::str::from_utf8(&patch).is_ok()) {
      let headers = &patch[..first];
      if headers
        .split(|byte| *byte == b'\n')
        .any(|line| line.starts_with(b"new file mode") || line.starts_with(b"deleted file mode") || line.starts_with(b"old mode"))
      {
        Vec::new()
      } else {
        starts.push(patch.len());
        starts.windows(2).map(|pair| pair[0]..pair[1]).collect()
      }
    } else {
      Vec::new()
    };
    Self { patch, ranges, untracked: false }
  }
}

fn diff(workdir: &Path, index: &Path, files: &[PathBuf], side: StageSide) -> Result<StageDiff, String> {
  let mut args: Vec<OsString> =
    ["--no-ext-diff", "--no-textconv", "--no-color", "--no-renames", "--full-index", "--src-prefix=a/", "--dst-prefix=b/", "-U3"]
      .into_iter()
      .map(OsString::from)
      .collect();
  if side == StageSide::Staged {
    args.push("--cached".into());
  }
  args.push("--".into());
  let specs = files.iter().map(|path| literal(path.as_os_str()));
  let output = run(workdir, index, "diff", args.into_iter().chain(specs.clone()))?;
  let mut diff = StageDiff::new(output.stdout, files.len() == 1);
  if side == StageSide::Unstaged && !diff.has_changes() {
    let untracked = run(workdir, index, "ls-files", ["--others", "--exclude-standard", "-z", "--"].into_iter().map(OsString::from).chain(specs))?;
    diff.untracked = !untracked.stdout.is_empty();
  }
  Ok(diff)
}

pub fn staging_diff(path: &Path, files: &[PathBuf], side: StageSide) -> Result<StageDiff, String> {
  validate_selection(files)?;
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let workdir = repository.workdir().ok_or("The repository has no working folder.")?;
  diff(workdir, &repository.path().join("index"), files, side)
}

/// Moves a whole file or one hunk between the staged and unstaged views. The working tree is untouched.
pub fn change_index(path: &Path, files: &[PathBuf], side: StageSide, hunk: Option<(&StageDiff, usize)>) -> Result<(), String> {
  validate_selection(files)?;
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let workdir = repository.workdir().ok_or("The repository has no working folder.")?;
  let index = IndexTransaction::open(&repository, workdir)?;
  crate::operation::require_idle(&repository)?;
  let head = Head::read(&repository)?;
  let shadow = &index.shadow;
  if let Some((expected, position)) = hunk {
    let current = diff(workdir, shadow, files, side)?;
    if current != *expected {
      return Err("The diff changed. Review the refreshed changes before trying again.".into());
    }
    let range = current.ranges.get(position).ok_or("That hunk is no longer available.")?;
    let header = current.ranges.first().ok_or("This change can only be staged as a whole file.")?.start;
    let patch_path = index.scratch.path().join("selection.patch");
    let mut patch = current.patch[..header].to_vec();
    patch.extend_from_slice(&current.patch[range.clone()]);
    fs::write(&patch_path, patch).map_err(|error| error.to_string())?;
    let mut arguments = vec![OsStr::new("--cached"), OsStr::new("--whitespace=nowarn")];
    if side == StageSide::Staged {
      arguments.push(OsStr::new("--reverse"));
    }
    arguments.push(patch_path.as_os_str());
    run(workdir, shadow, "apply", arguments)?;
  } else {
    let specs = files.iter().map(|path| literal(path.as_os_str()));
    match (side, head.tip) {
      (StageSide::Unstaged, _) => {
        let tracked = run(workdir, shadow, "ls-files", ["--cached", "-z", "--"].into_iter().map(OsString::from).chain(specs.clone()))?;
        let tracked: std::collections::HashSet<_> = crate::commit::paths(&tracked.stdout).collect();
        let mut paths = Vec::new();
        for (file, spec) in files.iter().zip(specs) {
          let known = tracked.contains(file.as_os_str());
          match fs::symlink_metadata(workdir.join(file)) {
            Ok(metadata) if metadata.is_dir() && !known => return Err("Select individual files to stage.".into()),
            Ok(_) => paths.push(spec),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
              if known {
                paths.push(spec);
              }
            }
            Err(error) => return Err(error.to_string()),
          }
        }
        if !paths.is_empty() {
          run(workdir, shadow, "add", ["--all", "--"].into_iter().map(OsString::from).chain(paths))?;
        }
      }
      (StageSide::Staged, Some(tip)) => {
        run(workdir, shadow, "reset", [OsString::from("--quiet"), tip.to_string().into(), "--".into()].into_iter().chain(specs))?;
      }
      (StageSide::Staged, None) => {
        run(workdir, shadow, "rm", ["--cached", "--force", "--ignore-unmatch", "--"].into_iter().map(OsString::from).chain(specs))?;
      }
    }
  }
  if Head::read(&repository)? != head {
    return Err("HEAD changed. Review the refreshed changes before trying again.".into());
  }
  crate::operation::require_idle(&repository)?;
  index.publish()
}

fn validate_selection(files: &[PathBuf]) -> Result<(), String> {
  validate_paths(files)?;
  if files.is_empty() {
    Err("Select a file first.".into())
  } else {
    Ok(())
  }
}
