//! Finding the next place a selected text occurs, for "Add Next Occurrence" (⌘D). Works on plain text and byte offsets.

use std::ops::Range;

fn is_word(character: char) -> bool {
  character.is_alphanumeric() || character == '_'
}

/// The word the offset is in or touches, or `None` when it is between non-word characters.
pub(crate) fn word_at(text: &str, offset: usize) -> Option<Range<usize>> {
  let offset = offset.min(text.len());
  let start = text[..offset].char_indices().rev().take_while(|(_, character)| is_word(*character)).last().map_or(offset, |(index, _)| index);
  let end = text[offset..].char_indices().find(|(_, character)| !is_word(*character)).map_or(text.len(), |(index, _)| offset + index);
  (start < end).then_some(start..end)
}

/// The next occurrence of `needle` at or after `from`, continuing from the top of the text when there is none below.
pub(crate) fn next_occurrence(text: &str, needle: &str, from: usize) -> Option<Range<usize>> {
  if needle.is_empty() {
    return None;
  }
  let from = from.min(text.len());
  let found = text[from..].find(needle).map(|index| from + index).or_else(|| text.find(needle))?;
  Some(found..found + needle.len())
}
