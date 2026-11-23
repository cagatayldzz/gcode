use ws_index::fuzzy::{rank, score};

#[test]
fn letters_must_appear_in_order() {
  assert!(score("mrs", "src/main.rs").is_some());
  assert!(score("smr", "src/main.rs").is_some());
  assert!(score("rsm", "src/main.rs").is_none());
}

#[test]
fn case_is_ignored() {
  assert_eq!(score("README", "docs/readme.md"), score("readme", "docs/README.md"));
  assert!(score("readme", "docs/README.md").is_some());
}

#[test]
fn an_empty_query_matches_everything() {
  assert_eq!(score("", "anything"), Some(0));
  assert_eq!(rank("  ", ["a", "b", "c"], 2), vec![0, 1]);
}

#[test]
fn a_query_longer_than_the_path_does_not_match() {
  assert!(score("abcdef", "abc").is_none());
}

#[test]
fn the_file_name_beats_a_match_scattered_over_folders() {
  let paths = ["crates/ui/src/workspace/entries.rs", "crates/ui/src/editor.rs", "docs/e/d/i/t/or.md"];
  assert_eq!(rank("editor", paths, 3)[0], 1);
}

#[test]
fn consecutive_letters_and_word_starts_win() {
  let paths = ["src/project_settings.rs", "src/pull_request.rs", "src/previous_snapshot_state.rs"];
  assert_eq!(rank("ps", paths, 3)[0], 0);
}

#[test]
fn a_shorter_path_wins_a_tie() {
  let paths = ["a/very/long/folder/name/lib.rs", "lib.rs"];
  assert_eq!(rank("lib", paths, 2), vec![1, 0]);
}

#[test]
fn the_limit_cuts_the_list() {
  let paths = ["a.rs", "ab.rs", "abc.rs", "abcd.rs"];
  assert_eq!(rank("a", paths, 2).len(), 2);
}

#[test]
fn a_capital_after_a_small_letter_starts_a_word() {
  assert!(score("fb", "FooBar.rs") > score("fb", "fabulous.rs"));
}

#[test]
fn non_matching_paths_are_left_out() {
  assert_eq!(rank("zzz", ["a.rs", "b.rs"], 5), Vec::<usize>::new());
}
