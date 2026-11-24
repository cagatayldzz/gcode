//! Line changes between Git snapshots and an editor buffer.

use git2::{ApplyLocation, ApplyOptions, Diff, DiffOptions, Patch, Repository};
use std::{ops::Range, path::Path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorChange {
  pub lines: Range<usize>,
  pub removed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditorDiff {
  pub changes: Vec<EditorChange>,
  pub unstaged: String,
  pub staged: String,
}

pub fn editor_diff(file: &Path, text: &str) -> Result<EditorDiff, git2::Error> {
  let metadata = std::fs::symlink_metadata(file).map_err(|error| git2::Error::from_str(&error.to_string()))?;
  if !metadata.is_file() {
    return Err(git2::Error::from_str("Git line changes are available only for regular files."));
  }
  let canonical = file.canonicalize().map_err(|error| git2::Error::from_str(&error.to_string()))?;
  let file = canonical.as_path();
  let repository = Repository::discover(file.parent().unwrap_or(file))?;
  let Some(relative) = repository.workdir().and_then(|root| file.strip_prefix(root).ok()) else {
    return Ok(EditorDiff::default());
  };
  let index = repository.index()?;
  let entry = index.get_path(relative, 0);
  if entry.is_none() && (index.get_path(relative, 1).is_some() || repository.status_should_ignore(relative)?) {
    return Ok(EditorDiff::default());
  }
  let indexed = entry.map(|entry| repository.find_blob(entry.id)).transpose()?;
  let head = repository.head().ok().and_then(|head| head.peel_to_tree().ok());
  let committed = head.and_then(|tree| tree.get_path(relative).ok()).and_then(|entry| repository.find_blob(entry.id()).ok());
  let indexed = indexed.as_ref().map_or(&[][..], |blob| blob.content());
  let committed = committed.as_ref().map_or(&[][..], |blob| blob.content());
  let mut options = DiffOptions::new();
  let baseline = if indexed == text.as_bytes() { committed } else { indexed };
  let markers = Patch::from_buffers(baseline, Some(relative), text.as_bytes(), Some(relative), Some(options.context_lines(0)))?;
  let changes = (0..markers.num_hunks())
    .map(|index| {
      let (hunk, _) = markers.hunk(index)?;
      let added = hunk.new_lines() as usize;
      let start = hunk.new_start() as usize;
      let start = if added == 0 { start } else { start.saturating_sub(1) };
      Ok(EditorChange { lines: start..start + added, removed: hunk.old_lines() > 0 })
    })
    .collect::<Result<_, git2::Error>>()?;
  let preview = |old: &[u8], new: &[u8]| -> Result<String, git2::Error> {
    if old == new {
      return Ok(String::new());
    }
    let mut patch = Patch::from_buffers(old, Some(relative), new, Some(relative), None)?;
    Ok(String::from_utf8_lossy(&patch.to_buf()?).into_owned())
  };
  Ok(EditorDiff { changes, unstaged: preview(indexed, text.as_bytes())?, staged: preview(committed, indexed)? })
}

#[derive(Clone, Copy)]
pub enum HunkAction {
  Stage,
  Unstage,
  Revert,
}

/// Applies only the single hunk intersecting the selected source lines.
pub fn apply_editor_hunk(file: &Path, expected: &str, lines: Range<usize>, original_side: bool, action: HunkAction) -> Result<(), git2::Error> {
  let stale = || git2::Error::from_str("The file or Git index changed. Open the diff again.");
  let metadata = std::fs::symlink_metadata(file).map_err(|error| git2::Error::from_str(&error.to_string()))?;
  if !metadata.is_file() {
    return Err(git2::Error::from_str("Git line changes are available only for regular files."));
  }
  let canonical = file.canonicalize().map_err(|error| git2::Error::from_str(&error.to_string()))?;
  let file = canonical.as_path();
  let repository = Repository::discover(file.parent().unwrap_or(file))?;
  let relative = repository.workdir().and_then(|root| file.strip_prefix(root).ok()).ok_or_else(stale)?;
  let disk = std::fs::read_to_string(file).map_err(|error| git2::Error::from_str(&error.to_string()))?;
  let index = repository.index()?;
  let index_entry = index.get_path(relative, 0);
  let index_blob = index_entry.as_ref().map(|entry| repository.find_blob(entry.id)).transpose()?;
  let indexed = index_blob.as_ref().map_or(&[][..], |blob| blob.content());
  let head = repository.head().ok().and_then(|head| head.peel_to_tree().ok());
  let head_blob = head.and_then(|tree| tree.get_path(relative).ok()).and_then(|entry| repository.find_blob(entry.id()).ok());
  let committed = head_blob.as_ref().map_or(&[][..], |blob| blob.content());
  let (old, new) = match action {
    HunkAction::Unstage => (committed, indexed),
    HunkAction::Stage | HunkAction::Revert => (indexed, disk.as_bytes()),
  };
  if index_entry.is_none() && (1..=3).any(|stage| index.get_path(relative, stage).is_some()) {
    return Err(stale());
  }
  let mut forward = Patch::from_buffers(old, Some(relative), new, Some(relative), None)?;
  if expected.is_empty() || forward.to_buf()?.as_ref() != expected.as_bytes() {
    return Err(stale());
  }
  let mut matching = Vec::new();
  for index in 0..forward.num_hunks() {
    let (_, count) = forward.hunk(index)?;
    for offset in 0..count {
      let line = forward.line_in_hunk(index, offset)?;
      let number = match (original_side, line.origin()) {
        (true, '-') => line.old_lineno(),
        (false, '+') => line.new_lineno(),
        _ => None,
      };
      if number.is_some_and(|number| lines.contains(&(number as usize))) {
        matching.push(index);
        break;
      }
    }
  }
  let [selected] = matching.as_slice() else {
    return Err(git2::Error::from_str("Select a line in a single changed block."));
  };

  if index_blob.is_none() && matches!(action, HunkAction::Stage) {
    let mut index = index;
    index.add_path(relative)?;
    return index.write();
  }
  if head_blob.is_none() && matches!(action, HunkAction::Unstage) {
    let mut index = index;
    index.remove_path(relative)?;
    return index.write();
  }
  let (location, reverse) = match action {
    HunkAction::Stage => (ApplyLocation::Index, false),
    HunkAction::Unstage => (ApplyLocation::Index, true),
    HunkAction::Revert => (ApplyLocation::WorkDir, true),
  };
  let (source, _) = forward.hunk(*selected)?;
  let source_coordinates = if reverse {
    (source.new_start(), source.old_start(), source.new_lines(), source.old_lines())
  } else {
    (source.old_start(), source.new_start(), source.old_lines(), source.new_lines())
  };
  let mut options = DiffOptions::new();
  let mut patch = Patch::from_buffers(old, Some(relative), new, Some(relative), Some(options.reverse(reverse)))?;
  let (hunk, _) = patch.hunk(*selected)?;
  if (hunk.old_start(), hunk.new_start(), hunk.old_lines(), hunk.new_lines()) != source_coordinates {
    return Err(git2::Error::from_str("This block could not be matched in the reverse diff."));
  }
  let coordinates = (hunk.old_start(), hunk.new_start());
  let buffer = patch.to_buf()?;
  let mut mode = index_entry.as_ref().map_or(0o100644, |entry| entry.mode);
  #[cfg(unix)]
  if matches!(action, HunkAction::Revert) {
    use std::os::unix::fs::MetadataExt as _;
    mode = if metadata.mode() & 0o100 != 0 { 0o100755 } else { 0o100644 };
  }
  // Buffer diffs assume 100644. Apply with the destination's mode so a text edit cannot change executable permissions.
  let text = std::str::from_utf8(&buffer).map_err(|_| git2::Error::from_str("The patch is not valid UTF-8."))?;
  let text = if mode == 0o100755 {
    let index_line = text.lines().take_while(|line| !line.starts_with("@@")).find(|line| line.starts_with("index "));
    index_line.map_or_else(|| text.to_owned(), |line| text.replacen(line, &line.replace(" 100644", " 100755"), 1))
  } else {
    text.to_owned()
  };
  let diff = Diff::from_buffer(text.as_bytes())?;
  let mut options = ApplyOptions::new();
  options.hunk_callback(|hunk| hunk.is_some_and(|hunk| (hunk.old_start(), hunk.new_start()) == coordinates));
  repository.apply(&diff, location, Some(&mut options))
}
