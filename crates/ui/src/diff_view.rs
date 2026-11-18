//! A diff as the Changes and History views show it: the Kit `Diff` with whole-line red and green backgrounds from the
//! theme's palette (`Colors::diff_added`, `Colors::diff_removed`), like GitHub Desktop, instead of a tint of the status colors.

use crate::theme::colors;
use gpui_kit::{
  component::diff::{Diff, DiffDecoration, DiffFile, DiffLineRange, DiffMode, DiffSide, DiffState},
  *,
};

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

/// The lines a unified patch removes (old numbers) and adds (new numbers), grouped into runs, by file in patch order.
fn changed_ranges(patch: &str, files: &[DiffFile]) -> Vec<ChangedRange> {
  let mut ranges: Vec<ChangedRange> = Vec::new();
  let mut file: Option<usize> = None;
  let mut in_hunk = false;
  let (mut old, mut new) = (0usize, 0usize);
  let mut note = |file: Option<usize>, side: DiffSide, line: usize| {
    let Some(path) = file.and_then(|index| files.get(index)).map(|file| file.path().clone()) else { return };
    match ranges.last_mut() {
      Some(last) if last.path == path && last.side == side && last.end + 1 == line => last.end = line,
      _ => ranges.push(ChangedRange { path, side, start: line, end: line }),
    }
  };
  for text in patch.lines() {
    if text.starts_with("diff --git ") {
      file = Some(file.map_or(0, |index| index + 1));
      in_hunk = false;
    } else if text.starts_with("@@") {
      // "@@ -old,count +new,count @@"
      let mut numbers =
        text.split_whitespace().skip(1).map(|part| part.trim_start_matches(['-', '+']).split(',').next().and_then(|n| n.parse::<usize>().ok()));
      (old, new) = (numbers.next().flatten().unwrap_or(0), numbers.next().flatten().unwrap_or(0));
      in_hunk = true;
      if file.is_none() {
        file = Some(0);
      }
    } else if in_hunk {
      match text.as_bytes().first() {
        Some(b'-') => {
          note(file, DiffSide::Original, old);
          old += 1;
        }
        Some(b'+') => {
          note(file, DiffSide::Modified, new);
          new += 1;
        }
        Some(b'\\') => {}
        _ => {
          old += 1;
          new += 1;
        }
      }
    }
  }
  ranges
}
