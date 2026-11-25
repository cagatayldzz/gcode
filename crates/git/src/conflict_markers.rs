//! Conflict regions (`<<<<<<<` … `>>>>>>>`) in the text of a file that a merge left in the working tree.

use std::ops::Range;

/// One conflict: the whole marked block, and the lines of the two sides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictRegion {
  /// Zero-based lines from the opening marker to the closing marker, both included.
  pub lines: Range<usize>,
  /// The lines kept from the current branch, between the opening marker and the separator.
  pub ours: Range<usize>,
  /// The lines coming from the other branch, between the separator and the closing marker.
  pub theirs: Range<usize>,
}

/// Finds the complete conflict blocks of `text`. A block that is not closed, or that is nested in another, is skipped.
pub fn conflict_regions(text: &str) -> Vec<ConflictRegion> {
  let mut regions = Vec::new();
  let mut open: Option<(usize, Option<usize>)> = None;
  for (line, content) in text.lines().enumerate() {
    if content.starts_with("<<<<<<<") {
      open = Some((line, None));
    } else if content.starts_with("=======") && content.trim_end() == "=======" {
      if let Some((_, separator @ None)) = open.as_mut() {
        *separator = Some(line);
      }
    } else if content.starts_with(">>>>>>>") {
      if let Some((start, Some(separator))) = open.take() {
        regions.push(ConflictRegion { lines: start..line + 1, ours: start + 1..separator, theirs: separator + 1..line });
      }
      open = None;
    }
  }
  regions
}
