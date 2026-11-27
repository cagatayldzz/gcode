use lsp_types::{Position, Range, TextEdit};
use ws_language::apply_text_edits;

fn edit(row: u32, from: u32, to: u32, replacement: &str) -> TextEdit {
  TextEdit::new(Range::new(Position::new(row, from), Position::new(row, to)), replacement.to_owned())
}

#[test]
fn edits_use_utf16_offsets_and_preserve_unmodified_crlf_lines() {
  let source = "🦀old\r\nsecond\r\n";
  let edits = [edit(1, 0, 6, "next"), edit(0, 2, 5, "fresh")];
  assert_eq!(apply_text_edits(source, &edits).unwrap(), "🦀fresh\r\nnext\r\n");
}

#[test]
fn a_surrogate_split_and_overlapping_edits_are_refused() {
  assert!(apply_text_edits("🦀text", &[edit(0, 1, 2, "bad")]).is_err());
  assert!(apply_text_edits("abcdef", &[edit(0, 0, 3, "one"), edit(0, 2, 4, "two")]).is_err());
}
