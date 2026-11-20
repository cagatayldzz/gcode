//! Bounded samples of buffer lines for the minimap.

use gpui_kit::base::input::{Rope, RopeExt as _};
use std::sync::Arc;

const MAX_SAMPLES: usize = 640;
pub(super) const MAX_COLUMNS: usize = 100;

#[derive(Clone, Copy)]
pub(super) struct Line {
  pub start: usize,
  pub end: usize,
  pub accent_end: usize,
}

pub(super) fn sample(text: &Rope) -> Arc<[Line]> {
  let count = text.lines_len();
  let samples = count.min(MAX_SAMPLES);
  (0..samples)
    .map(|index| {
      let mut start = MAX_COLUMNS;
      let mut end = 0;
      let mut accent_end = 0;
      for (column, character) in text.slice_line(index * count / samples).chars().take(MAX_COLUMNS).enumerate() {
        if !character.is_whitespace() {
          start = start.min(column);
          end = column + 1;
          if accent_end == column {
            accent_end = column + 1;
          }
        } else if end == 0 {
          accent_end = column + 1;
        }
      }
      Line { start, end, accent_end }
    })
    .collect()
}
