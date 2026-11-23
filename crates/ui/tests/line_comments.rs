#[path = "../src/line_comments.rs"]
mod line_comments;

use line_comments::*;

const HASH: (&str, &str) = ("# ", "");

fn apply(text: &str, selection: std::ops::Range<usize>, markers: (&str, &str), mode: CommentMode) -> Option<(String, std::ops::Range<usize>)> {
  let edit = edit(text, selection, markers, mode)?;
  let mut result = text.to_string();
  result.replace_range(edit.range.clone(), &edit.text);
  Some((result, edit.selection))
}

#[test]
fn adding_comments_covers_every_selected_line_at_the_shallowest_indent() {
  let text = "a\n  b\n\n  c\nd";
  let (result, selection) = apply(text, 0..text.len() - 2, HASH, CommentMode::Add).unwrap();
  assert_eq!(result, "# a\n#   b\n\n#   c\nd");
  assert_eq!(selection.start, 0);
}

#[test]
fn a_selection_ending_at_the_start_of_a_line_leaves_that_line_alone() {
  let (result, _) = apply("a\nb\nc", 0..4, HASH, CommentMode::Add).unwrap();
  assert_eq!(result, "# a\n# b\nc");
}

#[test]
fn removing_comments_takes_the_marker_and_one_space() {
  let (result, selection) = apply("# a\n#b\n  # c\nd", 0..12, HASH, CommentMode::Remove).unwrap();
  assert_eq!(result, "a\nb\n  c\nd");
  assert_eq!(selection.start, 0);
}

#[test]
fn toggle_removes_only_when_every_line_is_commented() {
  let (result, _) = apply("# a\n\n# b", 0..8, HASH, CommentMode::Toggle).unwrap();
  assert_eq!(result, "a\n\nb");
  let (result, _) = apply("# a\nb", 0..5, HASH, CommentMode::Toggle).unwrap();
  assert_eq!(result, "# # a\n# b");
}

#[test]
fn a_cursor_stays_in_its_word() {
  let (result, selection) = apply("let x = 1;", 6..6, ("// ", ""), CommentMode::Add).unwrap();
  assert_eq!(result, "// let x = 1;");
  assert_eq!(selection, 9..9);
  let (_, selection) = apply("// let x = 1;", 9..9, ("// ", ""), CommentMode::Remove).unwrap();
  assert_eq!(selection, 6..6);
}

#[test]
fn block_markers_wrap_each_line_and_come_off_again() {
  let (result, _) = apply("<p>\n<b>", 0..7, ("<!-- ", " -->"), CommentMode::Add).unwrap();
  assert_eq!(result, "<!-- <p> -->\n<!-- <b> -->");
  let (result, _) = apply(&result, 0..result.len(), ("<!-- ", " -->"), CommentMode::Remove).unwrap();
  assert_eq!(result, "<p>\n<b>");
}

#[test]
fn nothing_changes_when_there_is_no_comment_to_remove() {
  assert!(edit("a\nb", 0..3, HASH, CommentMode::Remove).is_none());
}

#[test]
fn a_line_ending_with_a_carriage_return_keeps_it() {
  let (result, _) = apply("a\r\nb\r\n", 0..4, HASH, CommentMode::Add).unwrap();
  assert_eq!(result, "# a\r\n# b\r\n");
}

#[test]
fn json_has_no_comments_but_the_env_files_do() {
  assert_eq!(markers("json"), None);
  assert_eq!(markers("dotenv"), Some(("# ", "")));
  assert_eq!(markers("rust"), Some(("// ", "")));
}
