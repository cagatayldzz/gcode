//! A diff as the Changes and History views show it: the Kit `Diff` with whole-line red and green backgrounds from the
//! theme's palette (`Colors::diff_added`, `Colors::diff_removed`), instead of a tint of the status colors.

use crate::{editor::language_for, theme::colors};
use gpui_kit::{
  component::{
    diff::{Diff, DiffDecoration, DiffFile, DiffLineRange, DiffMode, DiffSide, DiffState},
    highlighter::LanguageRegistry,
  },
  *,
};
use std::path::Path;

/// Parses a patch into files whose syntax is colored like the editor's: the Kit detects the language from the extension alone,
/// so the files it does not know (`Cargo.lock`, `.zsh`, `.yml`, ...) get the language the editor uses for them.
pub(crate) fn parse_patch(patch: &str) -> Result<Vec<DiffFile>, String> {
  let files = DiffFile::parse(patch).map_err(|error| error.to_string())?;
  Ok(files.into_iter().map(with_editor_language).collect())
}

fn with_editor_language(file: DiffFile) -> DiffFile {
  if file.language() != "text" {
    return file;
  }
  let path = Path::new(file.path().as_ref());
  let language = language_for(path);
  if language != "text" && LanguageRegistry::singleton().language(language).is_some() {
    file.with_language(language)
  } else {
    file
  }
}

/// A run of lines the patch adds or removes.
struct ChangedRange {
  path: SharedString,
  side: DiffSide,
  start: usize,
  end: usize,
}

/// A parsed patch with the lines it changes.
pub(crate) struct ShownDiff {
  state: Entity<DiffState>,
  changed: Vec<ChangedRange>,
}

impl ShownDiff {
  /// Shows `files`, which were parsed from `patch`.
  pub fn new(files: Vec<DiffFile>, patch: &str, cx: &mut App) -> Self {
    let changed = changed_ranges(patch, &files);
    let state = cx.new(|cx| DiffState::new(files, cx).with_mode(DiffMode::Unified));
    Self { state, changed }
  }

  pub fn set_files(&mut self, files: Vec<DiffFile>, patch: &str, cx: &mut App) {
    self.changed = changed_ranges(patch, &files);
    self.state.update(cx, |state, cx| state.set_files(files, cx));
  }

  pub fn state(&self) -> &Entity<DiffState> {
    &self.state
  }

  /// The Kit diff, to be given the sizes and options of the place that shows it.
  pub fn view(&self, cx: &App) -> Diff {
    let palette = colors(cx);
    let decorations = self.changed.iter().map(|range| {
      let background = if range.side == DiffSide::Original { palette.diff_removed } else { palette.diff_added };
      DiffDecoration::new(DiffLineRange::new(range.path.clone(), range.side, range.start, range.end)).with_background(background)
    });
    Diff::new(&self.state).change_background(false).decorations(decorations)
  }
}

/// Visits source lines in patch order. Context is represented by its modified-side line number.
pub(crate) fn visit_patch_lines(patch: &str, files: &[DiffFile], mut visit: impl FnMut(&DiffFile, DiffSide, usize, bool, usize)) {
  let mut files = files.iter();
  let mut file = files.next();
  let mut saw_file_header = false;
  let mut hunk = None;
  for text in patch.lines() {
    if text.starts_with("diff --git ") {
      if saw_file_header {
        file = files.next();
      }
      saw_file_header = true;
      hunk = None;
    } else if text.starts_with("@@ ") {
      hunk = hunk_start(text).map(|(old, new)| (old, new, 0));
    } else if let (Some(file), Some((old, new, row))) = (file, &mut hunk) {
      let (side, line, changed) = match text.as_bytes().first() {
        Some(b'-') => {
          let line = *old;
          *old += 1;
          (DiffSide::Original, line, true)
        }
        Some(b'+') => {
          let line = *new;
          *new += 1;
          (DiffSide::Modified, line, true)
        }
        Some(b' ') => {
          let line = *new;
          *old += 1;
          *new += 1;
          (DiffSide::Modified, line, false)
        }
        _ => continue,
      };
      visit(file, side, line, changed, *row);
      *row += 1;
    }
  }
}

fn hunk_start(header: &str) -> Option<(usize, usize)> {
  let mut ranges = header.split_whitespace().skip(1);
  let start = |range: &str, prefix| range.strip_prefix(prefix)?.split(',').next()?.parse::<usize>().ok();
  Some((start(ranges.next()?, '-')?, start(ranges.next()?, '+')?))
}

/// The added and removed lines, grouped into runs by file and side.
fn changed_ranges(patch: &str, files: &[DiffFile]) -> Vec<ChangedRange> {
  let mut ranges: Vec<ChangedRange> = Vec::new();
  visit_patch_lines(patch, files, |file, side, line, changed, _| {
    if !changed {
      return;
    }
    let path = file.path();
    match ranges.last_mut() {
      Some(last) if &last.path == path && last.side == side && last.end + 1 == line => last.end = line,
      _ => ranges.push(ChangedRange { path: path.clone(), side, start: line, end: line }),
    }
  });
  ranges
}
