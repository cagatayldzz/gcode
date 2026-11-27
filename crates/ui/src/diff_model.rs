//! The lines of one file's patch and how a diff table lays them out: which lines are shown, which runs of unchanged lines are folded
//! away, and which words of a changed line changed. No interface code lives here, so that it can be tested on its own.

use similar::{ChangeTag, TextDiff};
use std::{collections::HashMap, ops::Range};

/// How many unchanged lines stay around a change when the rest is folded.
pub(crate) const CONTEXT: usize = 3;

/// How many hidden lines one click on an expand arrow brings back.
pub(crate) const EXPAND_STEP: usize = 20;

/// The least similarity (0 to 1) two lines need before the words that differ between them are emphasized.
const MIN_SIMILARITY: f32 = 0.4;

/// The longest line whose words are compared; a longer one is emphasized as a whole line.
const MAX_COMPARED: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineKind {
  Context,
  Added,
  Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Line {
  pub kind: LineKind,
  /// The line number in the old file; none for an added line.
  pub old: Option<u32>,
  /// The line number in the new file; none for a removed line.
  pub new: Option<u32>,
  pub text: String,
}

/// A run of changed lines of a patch, and the lines it changes in the old and the new file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChangeGroup {
  pub lines: Range<usize>,
  pub old: Range<u32>,
  pub new: Range<u32>,
}

/// A patch of one file: its lines in order, and for each the byte ranges of its text that differ from the line it was paired with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Patch {
  pub lines: Vec<Line>,
  /// For each line, the words that differ from the line it was paired with, and the characters that do.
  pub emphasis: Vec<Vec<Range<usize>>>,
  pub char_emphasis: Vec<Vec<Range<usize>>>,
  /// The index in `lines` where each hunk starts.
  pub hunk_starts: Vec<usize>,
}

impl Patch {
  /// Reads the first file of a unified patch. A patch with no text hunks (binary files, mode changes) has no lines.
  pub fn parse(patch: &str) -> Self {
    let mut lines: Vec<Line> = Vec::new();
    let mut hunk_starts = Vec::new();
    let (mut old, mut new) = (0u32, 0u32);
    let mut in_hunk = false;
    for text in patch.split('\n') {
      if let Some(header) = text.strip_prefix("@@ ") {
        let Some((first_old, first_new)) = hunk_numbers(header) else { continue };
        (old, new) = (first_old, first_new);
        in_hunk = true;
        hunk_starts.push(lines.len());
        continue;
      }
      if !in_hunk {
        continue;
      }
      if text.starts_with("diff --git ") {
        break;
      }
      match text.as_bytes().first() {
        Some(b' ') => {
          lines.push(Line { kind: LineKind::Context, old: Some(old), new: Some(new), text: text[1..].trim_end_matches('\r').to_string() });
          old += 1;
          new += 1;
        }
        Some(b'-') => {
          lines.push(Line { kind: LineKind::Removed, old: Some(old), new: None, text: text[1..].trim_end_matches('\r').to_string() });
          old += 1;
        }
        Some(b'+') => {
          lines.push(Line { kind: LineKind::Added, old: None, new: Some(new), text: text[1..].trim_end_matches('\r').to_string() });
          new += 1;
        }
        _ => {}
      }
    }
    let emphasis = emphasize(&lines, Unit::Word);
    let char_emphasis = emphasize(&lines, Unit::Character);
    Self { lines, emphasis, char_emphasis, hunk_starts }
  }

  /// The digits of the longest line number, which sets the width of the number columns.
  pub fn digits(&self) -> usize {
    let widest = self.lines.iter().flat_map(|line| [line.old, line.new]).flatten().max().unwrap_or(1);
    widest.max(1).to_string().len()
  }

  /// The runs of changed lines, each with the lines it changes on both sides in half open ranges: the old range holds the removed
  /// lines (or the place the added ones come in), the new range the added lines (or the place the removed ones were). This is what
  /// a check box beside the diff stands for, since the patch of a file is one hunk with all of the file as context.
  pub fn change_groups(&self) -> Vec<ChangeGroup> {
    let mut groups = Vec::new();
    let mut start = None;
    for ix in 0..=self.lines.len() {
      let changed = self.lines.get(ix).is_some_and(|line| line.kind != LineKind::Context);
      match (start, changed) {
        (None, true) => start = Some(ix),
        (Some(first), false) => {
          groups.push(self.group(first..ix));
          start = None;
        }
        _ => {}
      }
    }
    groups
  }

  fn group(&self, lines: Range<usize>) -> ChangeGroup {
    let span = |number: fn(&Line) -> Option<u32>| {
      let own: Vec<u32> = self.lines[lines.clone()].iter().filter_map(number).collect();
      match (own.first(), own.last()) {
        (Some(first), Some(last)) => *first..*last + 1,
        _ => {
          let at = self.lines[lines.end..]
            .iter()
            .find_map(number)
            .or_else(|| self.lines[..lines.start].iter().rev().find_map(number).map(|before| before + 1))
            .unwrap_or(1);
          at..at
        }
      }
    };
    ChangeGroup { old: span(|line| line.old), new: span(|line| line.new), lines }
  }

  /// The lines of each hunk, as ranges of `lines`.
  pub fn hunks(&self) -> Vec<Range<usize>> {
    self.hunk_starts.iter().enumerate().map(|(ix, start)| *start..self.hunk_starts.get(ix + 1).copied().unwrap_or(self.lines.len())).collect()
  }

  /// The text of one side of the lines in `range`, a line ending after each line, as the highlighter should read it: the old side
  /// holds the context and removed lines, the new side the context and added ones. Also gives, for each line of the range, where its
  /// text starts in the result (none for a line that is not on that side).
  pub fn side_text(&self, range: Range<usize>, removed_side: bool) -> (String, Vec<Option<usize>>) {
    let mut text = String::new();
    let mut starts = Vec::with_capacity(range.len());
    for line in &self.lines[range] {
      let on_side = match line.kind {
        LineKind::Context => true,
        LineKind::Removed => removed_side,
        LineKind::Added => !removed_side,
      };
      if on_side {
        starts.push(Some(text.len()));
        text.push_str(&line.text);
        text.push('\n');
      } else {
        starts.push(None);
      }
    }
    (text, starts)
  }
}

/// The first old and new line numbers of a hunk header: `-3,7 +3,8 @@ scope` (what follows `@@ `).
fn hunk_numbers(header: &str) -> Option<(u32, u32)> {
  let (old, rest) = header.strip_prefix('-')?.split_once(" +")?;
  let new = rest.split_once(' ').map_or(rest, |(new, _)| new);
  let first = |range: &str| range.split_once(',').map_or(range, |(first, _)| first).parse::<u32>().ok();
  Some((first(old)?, first(new)?))
}

/// Compares each run of removed lines with the run of added lines right after it, line by line.
fn emphasize(lines: &[Line], unit: Unit) -> Vec<Vec<Range<usize>>> {
  let mut result = vec![Vec::new(); lines.len()];
  let mut start = 0;
  while start < lines.len() {
    if lines[start].kind != LineKind::Removed {
      start += 1;
      continue;
    }
    let removed_end = start + lines[start..].iter().take_while(|line| line.kind == LineKind::Removed).count();
    let added_end = removed_end + lines[removed_end..].iter().take_while(|line| line.kind == LineKind::Added).count();
    for offset in 0..(removed_end - start).min(added_end - removed_end) {
      let (old_ix, new_ix) = (start + offset, removed_end + offset);
      let (old_text, new_text) = (&lines[old_ix].text, &lines[new_ix].text);
      if old_text == new_text || old_text.len() > MAX_COMPARED || new_text.len() > MAX_COMPARED {
        continue;
      }
      let (old_runs, new_runs) = changed_runs(old_text, new_text, unit);
      result[old_ix] = old_runs;
      result[new_ix] = new_runs;
    }
    start = added_end.max(start + 1);
  }
  result
}

/// What is compared when two lines are emphasized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
  Word,
  Character,
}

/// The byte ranges that differ between two lines, on each side. The leading indentation is never part of them; changed words with only
/// whitespace between them are one run, and changed characters that touch are.
pub(crate) fn changed_runs(old: &str, new: &str, unit: Unit) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
  let diff = match unit {
    Unit::Word => TextDiff::from_unicode_words(old, new),
    Unit::Character => TextDiff::from_graphemes(old, new),
  };
  if diff.ratio() < MIN_SIMILARITY {
    return (Vec::new(), Vec::new());
  }
  let (mut old_runs, mut new_runs) = (Vec::new(), Vec::new());
  let (mut old_at, mut new_at) = (0, 0);
  for change in diff.iter_all_changes() {
    let length = change.value().len();
    match change.tag() {
      ChangeTag::Equal => {
        old_at += length;
        new_at += length;
      }
      ChangeTag::Delete => {
        push_run(&mut old_runs, old_at..old_at + length, old, unit);
        old_at += length;
      }
      ChangeTag::Insert => {
        push_run(&mut new_runs, new_at..new_at + length, new, unit);
        new_at += length;
      }
    }
  }
  (without_indent(old_runs, old), without_indent(new_runs, new))
}

fn push_run(runs: &mut Vec<Range<usize>>, range: Range<usize>, text: &str, unit: Unit) {
  if let Some(last) = runs.last_mut() {
    let gap = &text[last.end..range.start];
    if gap.is_empty() || (unit == Unit::Word && gap.trim().is_empty()) {
      last.end = range.end;
      return;
    }
  }
  runs.push(range);
}

fn without_indent(runs: Vec<Range<usize>>, text: &str) -> Vec<Range<usize>> {
  let indent = text.len() - text.trim_start().len();
  runs.into_iter().map(|run| run.start.max(indent)..run.end).filter(|run| run.start < run.end).collect()
}

/// Which side of a folded run the file goes on beyond: a run at the top has no lines above it, one at the bottom none below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edge {
  Top,
  Middle,
  Bottom,
}

/// One row of the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Row {
  /// A line of the patch, by its index in `Patch::lines`.
  Line(usize),
  /// Side by side: the old line on the left and the new one on the right, either of which may be missing.
  Pair(Option<usize>, Option<usize>),
  /// A run of unchanged lines that is not shown. `hidden` are their indexes; `base` is where the run started before anything
  /// was expanded, which is what identifies it; `label` is the `@@` header of the lines that follow it.
  Fold { hidden: Range<usize>, base: usize, edge: Edge, label: String },
}

/// How many lines of each fold were brought back, counted from the start of the fold and from its end, by the fold's `base`.
pub(crate) type Expansion = HashMap<usize, (usize, usize)>;

pub(crate) struct Layout {
  pub rows: Vec<Row>,
  /// The row where each run of changed lines starts.
  pub change_starts: Vec<usize>,
}

/// Lays the lines out: the changes with [`CONTEXT`] lines around them, a fold for each run of the other lines, and the lines that
/// were brought back out of a fold.
pub(crate) fn layout(patch: &Patch, expansion: &Expansion, split: bool) -> Layout {
  let lines = &patch.lines;
  let mut near_change = vec![false; lines.len()];
  for (ix, line) in lines.iter().enumerate() {
    if line.kind != LineKind::Context {
      for shown in near_change.iter_mut().take((ix + CONTEXT + 1).min(lines.len())).skip(ix.saturating_sub(CONTEXT)) {
        *shown = true;
      }
    }
  }
  let mut rows: Vec<Row> = Vec::new();
  let mut ix = 0;
  while ix < lines.len() {
    if near_change[ix] {
      rows.push(Row::Line(ix));
      ix += 1;
      continue;
    }
    let end = ix + near_change[ix..].iter().take_while(|near| !**near).count();
    let edge = match (ix == 0, end == lines.len()) {
      (true, _) => Edge::Top,
      (_, true) => Edge::Bottom,
      _ => Edge::Middle,
    };
    // A run of one or two lines is cheaper shown than folded.
    if end - ix < 3 && edge == Edge::Middle {
      rows.extend((ix..end).map(Row::Line));
      ix = end;
      continue;
    }
    let (from_start, from_end) = expansion.get(&ix).copied().unwrap_or((0, 0));
    let from_start = from_start.min(end - ix);
    let from_end = from_end.min(end - ix - from_start);
    rows.extend((ix..ix + from_start).map(Row::Line));
    let hidden = ix + from_start..end - from_end;
    if !hidden.is_empty() {
      rows.push(Row::Fold { hidden, base: ix, edge, label: String::new() });
    }
    rows.extend((end - from_end..end).map(Row::Line));
    ix = end;
  }
  // The label of a fold is the header of the lines shown right after it.
  for row_ix in 0..rows.len() {
    if let Row::Fold { edge, .. } = &rows[row_ix] {
      if *edge == Edge::Bottom {
        continue;
      }
      let following: Vec<&Line> =
        rows[row_ix + 1..].iter().map_while(|row| if let Row::Line(line) = row { Some(&lines[*line]) } else { None }).collect();
      let header = block_header(&following);
      if let Row::Fold { label, .. } = &mut rows[row_ix] {
        *label = header;
      }
    }
  }
  if split {
    rows = side_by_side(rows, lines);
  }
  let mut change_starts = Vec::new();
  let mut previous_changed = false;
  for (row_ix, row) in rows.iter().enumerate() {
    let changed = match row {
      Row::Line(line) => lines[*line].kind != LineKind::Context,
      Row::Pair(old, new) => [old, new].into_iter().flatten().any(|line| lines[*line].kind != LineKind::Context),
      Row::Fold { .. } => false,
    };
    if changed && !previous_changed {
      change_starts.push(row_ix);
    }
    previous_changed = changed;
  }
  Layout { rows, change_starts }
}

/// Puts each run of removed lines beside the run of added lines that follows it; unchanged lines stand on both sides.
fn side_by_side(rows: Vec<Row>, lines: &[Line]) -> Vec<Row> {
  let mut result = Vec::with_capacity(rows.len());
  let mut ix = 0;
  while ix < rows.len() {
    let Row::Line(line) = rows[ix] else {
      result.push(rows[ix].clone());
      ix += 1;
      continue;
    };
    match lines[line].kind {
      LineKind::Context => {
        result.push(Row::Pair(Some(line), Some(line)));
        ix += 1;
      }
      _ => {
        let run = |kind: LineKind, from: usize| -> Vec<usize> {
          rows[from..]
            .iter()
            .map_while(|row| if let Row::Line(line) = row { Some(*line) } else { None })
            .take_while(|line| lines[*line].kind == kind)
            .collect()
        };
        let removed = run(LineKind::Removed, ix);
        let added = run(LineKind::Added, ix + removed.len());
        for pair in 0..removed.len().max(added.len()) {
          result.push(Row::Pair(removed.get(pair).copied(), added.get(pair).copied()));
        }
        ix += removed.len() + added.len();
      }
    }
  }
  result
}

/// `@@ -old,count +new,count @@` for a block of lines shown together.
fn block_header(block: &[&Line]) -> String {
  let first = |pick: fn(&Line) -> Option<u32>| block.iter().find_map(|line| pick(line)).unwrap_or(0);
  let count = |pick: fn(&Line) -> Option<u32>| block.iter().filter(|line| pick(line).is_some()).count();
  format!("@@ -{},{} +{},{} @@", first(|line| line.old), count(|line| line.old), first(|line| line.new), count(|line| line.new))
}

/// What one click on an expand arrow does to the fold it is on: `up` brings back the lines next to the lines below the fold, otherwise
/// those next to the lines above it.
pub(crate) fn expand(expansion: &mut Expansion, base: usize, hidden: &Range<usize>, up: bool) {
  let (from_start, from_end) = expansion.entry(base).or_insert((0, 0));
  let step = EXPAND_STEP.min(hidden.len());
  if up {
    *from_end += step;
  } else {
    *from_start += step;
  }
}
