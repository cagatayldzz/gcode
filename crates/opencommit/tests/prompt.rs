use ws_opencommit::{estimate_tokens, parse_message, split_diff, system_prompt, Config};

#[test]
fn the_default_prompt_asks_for_one_conventional_line_without_scope() {
  let prompt = system_prompt(&Config::default(), "");
  assert!(prompt.contains("Conventional Commits"));
  assert!(prompt.contains("one single line"));
  assert!(prompt.contains("Do not put a scope"));
  assert!(prompt.contains("in English"));
  assert!(!prompt.contains("GitMoji"));
  assert!(!prompt.contains("Additional instructions"));
}

#[test]
fn settings_change_the_prompt() {
  let config = Config { one_line: false, description: true, why: true, emoji: true, omit_scope: false, language: "tr".into(), ..Config::default() };
  let prompt = system_prompt(&config, "  Keep it short.  ");
  assert!(prompt.contains("empty line and a short body"));
  assert!(prompt.contains("what changed"));
  assert!(prompt.contains("why the change was made"));
  assert!(prompt.contains("GitMoji"));
  assert!(!prompt.contains("Do not put a scope"));
  assert!(prompt.contains("in Türkçe"));
  assert!(prompt.ends_with("Keep it short."));
}

#[test]
fn a_small_diff_stays_in_one_part() {
  let diff = "diff --git a/a b/a\n+one\ndiff --git a/b b/b\n+two\n";
  assert_eq!(split_diff(diff, 1000), vec![diff.to_string()]);
}

#[test]
fn a_large_diff_is_split_between_files_never_inside_one() {
  let file = |name: &str| format!("diff --git a/{name} b/{name}\n+{}\n", "x".repeat(400));
  let diff = format!("{}{}{}", file("a"), file("b"), file("c"));
  let parts = split_diff(&diff, 150);
  assert_eq!(parts.len(), 3);
  for (part, name) in parts.iter().zip(["a", "b", "c"]) {
    assert!(part.starts_with(&format!("diff --git a/{name}")));
  }
}

#[test]
fn a_file_larger_than_the_limit_is_truncated_and_marked() {
  let lines: String = (0..500).map(|index| format!("+line number {index}\n")).collect();
  let parts = split_diff(&format!("diff --git a/big b/big\n{lines}"), 200);
  assert_eq!(parts.len(), 1);
  assert!(parts[0].ends_with("[diff truncated]\n"));
  assert!(estimate_tokens(&parts[0]) <= 200);
}

#[test]
fn an_empty_diff_has_no_parts() {
  assert!(split_diff("  \n", 100).is_empty());
}

#[test]
fn the_first_line_is_the_summary_and_the_rest_the_description() {
  let message = parse_message("feat: add search\n\nIt finds files.\nIt is fast.\n", false);
  assert_eq!(message.summary, "feat: add search");
  assert_eq!(message.description, "It finds files.\nIt is fast.");
}

#[test]
fn one_line_mode_drops_the_description() {
  let message = parse_message("feat: add search\n\nextra", true);
  assert_eq!((message.summary.as_str(), message.description.as_str()), ("feat: add search", ""));
}

#[test]
fn fences_and_quotes_around_the_answer_are_removed() {
  assert_eq!(parse_message("```\nfix: a crash\n```", true).summary, "fix: a crash");
  assert_eq!(parse_message("\"fix: a crash\"", true).summary, "fix: a crash");
}
