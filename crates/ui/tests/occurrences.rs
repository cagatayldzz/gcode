#[path = "../src/occurrences.rs"]
mod occurrences;

use occurrences::*;

#[test]
fn the_word_under_the_cursor_includes_its_edges() {
  let text = "let foo_bar = 1;";
  assert_eq!(word_at(text, 6), Some(4..11));
  assert_eq!(word_at(text, 4), Some(4..11));
  assert_eq!(word_at(text, 11), Some(4..11));
  assert_eq!(word_at("a  b", 2), None);
}

#[test]
fn the_next_occurrence_follows_the_cursor_and_wraps_around() {
  let text = "ab x ab y ab";
  assert_eq!(next_occurrence(text, "ab", 2), Some(5..7));
  assert_eq!(next_occurrence(text, "ab", 7), Some(10..12));
  assert_eq!(next_occurrence(text, "ab", 12), Some(0..2));
  assert_eq!(next_occurrence(text, "zz", 0), None);
  assert_eq!(next_occurrence(text, "", 0), None);
}

#[test]
fn offsets_stay_on_character_boundaries() {
  let text = "çağ çağ";
  assert_eq!(word_at(text, 2), Some(0..5));
  assert_eq!(next_occurrence(text, "çağ", 5), Some(6..11));
}
