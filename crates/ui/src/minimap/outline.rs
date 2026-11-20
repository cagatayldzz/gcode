//! Bounded samples of buffer lines for the minimap.

use gpui_kit::{
  base::input::{HighlightStyleResolver, Rope, RopeExt as _},
  component::highlighter::{HighlightTheme, SyntaxHighlighter},
  HighlightStyle, Hsla, SharedString,
};
use std::{ops::Range, sync::Arc};

const MAX_SAMPLES: usize = 640;
const MAX_COLUMNS: usize = 100;

pub(super) struct Line {
  pub text: SharedString,
  pub colors: Vec<(usize, Option<Hsla>)>,
}

pub(super) fn sample(text: &Rope, language: &str, theme: &HighlightTheme) -> Arc<[Line]> {
  let highlighter = (language != "gitignore").then(|| {
    let mut highlighter = SyntaxHighlighter::new(language);
    highlighter.update(None, text, None);
    highlighter
  });
  let count = text.lines_len();
  let samples = count.min(MAX_SAMPLES);
  (0..samples)
    .map(|index| {
      let row = index * count / samples;
      let mut line = String::new();
      for character in text.slice_line(row).chars().take(MAX_COLUMNS) {
        match character {
          '\r' | '\n' => break,
          '\t' => line.push(' '),
          _ => line.push(character),
        }
      }
      let line = line.trim_end();
      let start = text.line_start_offset(row);
      let colors = if line.is_empty() {
        Vec::new()
      } else {
        let range = start..start + line.len();
        let styles = match &highlighter {
          Some(highlighter) => highlighter.styles(&range, theme),
          None => gitignore_styles(text, &range, theme),
        };
        styles.into_iter().map(|(range, style)| (range.len(), style.color)).collect()
      };
      Line { text: SharedString::from(line.to_owned()), colors }
    })
    .collect()
}

/// Git treats a hash as a comment only at the beginning of a line.
pub(crate) fn gitignore_styles(text: &Rope, range: &Range<usize>, resolver: &dyn HighlightStyleResolver) -> Vec<(Range<usize>, HighlightStyle)> {
  let comment = resolver.style("comment").unwrap_or_default();
  let start_row = text.offset_to_point(range.start).row;
  let end_row = text.offset_to_point(range.end).row;
  (start_row..=end_row)
    .filter_map(|row| {
      let line_start = text.line_start_offset(row);
      let start = range.start.max(line_start);
      let end = range.end.min(text.line_end_offset(row));
      let style = if text.char_at(line_start) == Some('#') { comment } else { HighlightStyle::default() };
      (start < end).then_some((start..end, style))
    })
    .collect()
}
