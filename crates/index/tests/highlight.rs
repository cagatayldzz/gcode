use std::ops::Range;
use vekro_index::highlight::{Highlights, Language, Role, Run};

const ENV: &str = "# a comment\nNEXT_PUBLIC_TOKEN=abc/def+ghi==\nexport MODE=production\n\n  ## Endpoint: https://test.example.cloud\nURL=https://www.example.com/path\n# OLD_KEY=https://old.example.com\r\nLAST=1";

fn covers(runs: &[Run], range: &Range<usize>) {
  let mut cursor = range.start;
  for (run, _) in runs {
    assert_eq!(run.start, cursor, "a run starts where the previous one ended");
    assert!(run.end > run.start, "no empty runs");
    cursor = run.end;
  }
  assert_eq!(cursor, range.end, "the runs reach the end of the range");
}

fn role_at(runs: &[Run], text: &str, needle: &str) -> Option<Role> {
  let offset = text.find(needle).unwrap();
  runs.iter().find(|(run, _)| run.contains(&offset)).and_then(|(_, role)| *role)
}

fn text_of<'a>(runs: &[Run], text: &'a str, needle: &str) -> &'a str {
  let offset = text.find(needle).unwrap();
  let (run, _) = runs.iter().find(|(run, _)| run.contains(&offset)).unwrap();
  &text[run.clone()]
}

#[test]
fn the_runs_cover_the_whole_text_newlines_included() {
  for text in [ENV, "", "\n", "\n\n", "KEY=1\n", "x".repeat(5000).as_str(), "# only a comment"] {
    let highlights = Highlights::compute(Language::Dotenv, text);
    assert_eq!(highlights.text_len(), text.len());
    covers(highlights.runs(), &(0..text.len()));
    let gitignore = Highlights::compute(Language::Gitignore, text);
    covers(gitignore.runs(), &(0..text.len()));
  }
}

#[test]
fn dotenv_names_comments_keywords_and_addresses_have_their_roles() {
  let runs = Highlights::compute(Language::Dotenv, ENV).runs().to_vec();
  assert_eq!(role_at(&runs, ENV, "# a comment"), Some(Role::Comment));
  assert_eq!(role_at(&runs, ENV, "# OLD_KEY"), Some(Role::Comment), "a setting in a comment is a comment");
  assert_eq!(text_of(&runs, ENV, "NEXT_PUBLIC_TOKEN"), "NEXT_PUBLIC_TOKEN");
  assert_eq!(role_at(&runs, ENV, "NEXT_PUBLIC_TOKEN"), Some(Role::Name));
  assert_eq!(role_at(&runs, ENV, "abc/def+ghi=="), None, "a value is plain text");
  assert_eq!(role_at(&runs, ENV, "export"), Some(Role::Keyword));
  assert_eq!(role_at(&runs, ENV, "MODE"), Some(Role::Name));
  assert_eq!(role_at(&runs, ENV, "LAST"), Some(Role::Name), "the last line needs no newline");
  assert_eq!(role_at(&runs, ENV, "https://www.example.com"), Some(Role::Link));
  assert_eq!(text_of(&runs, ENV, "https://www.example.com"), "https://www.example.com/path");
  assert_eq!(role_at(&runs, ENV, "https://old.example.com"), Some(Role::CommentLink));
  assert_eq!(role_at(&runs, ENV, "https://test.example.cloud"), Some(Role::CommentLink), "indented comments count too");
  assert_eq!(text_of(&runs, ENV, "https://test.example.cloud"), "https://test.example.cloud");
}

#[test]
fn a_carriage_return_is_not_part_of_a_value_or_an_address() {
  let text = "A=https://a.example\r\nB=2\r\n";
  let runs = Highlights::compute(Language::Dotenv, text).runs().to_vec();
  assert_eq!(text_of(&runs, text, "https://a.example"), "https://a.example");
}

#[test]
fn gitignore_marks_only_a_hash_at_the_start_of_a_line() {
  let text = "# header\n/build\nfile#not-a-comment\n  # indented is a pattern\n";
  let runs = Highlights::compute(Language::Gitignore, text).runs().to_vec();
  assert_eq!(role_at(&runs, text, "# header"), Some(Role::Comment));
  assert_eq!(role_at(&runs, text, "/build"), None);
  assert_eq!(role_at(&runs, text, "#not-a-comment"), None);
  assert_eq!(role_at(&runs, text, "# indented"), None);
}

#[test]
fn a_slice_is_cut_at_the_edges_and_still_covers_its_range() {
  let highlights = Highlights::compute(Language::Dotenv, ENV);
  let start = ENV.find("URL=").unwrap();
  for range in [0..ENV.len(), start..start + 3, 0..5, 3..ENV.len() - 4, start + 1..start + 2, ENV.len()..ENV.len()] {
    let runs = highlights.slice(&range);
    if range.is_empty() {
      assert!(runs.is_empty());
    } else {
      covers(&runs, &range);
    }
  }
  assert_eq!(highlights.slice(&(ENV.len() - 2..ENV.len() + 10)).last().unwrap().0.end, ENV.len() + 10, "past the end is plain");
}

#[test]
fn an_oversized_text_is_left_plain_instead_of_costing_time() {
  let text = "KEY=value\n".repeat(vekro_index::highlight::MAX_BYTES / 10 + 10);
  let highlights = Highlights::compute(Language::Dotenv, &text);
  assert_eq!(highlights.runs().len(), 1);
  assert_eq!(highlights.runs()[0], (0..text.len(), None));
}

#[test]
fn languages_are_found_by_file_name() {
  assert_eq!(Language::for_file_name(".gitignore"), Some(Language::Gitignore));
  for name in [".env", ".env.local", ".env.production", "production.env"] {
    assert_eq!(Language::for_file_name(name), Some(Language::Dotenv), "{name}");
  }
  for name in ["env", "main.rs", ".environment", "envfile", ".gitignore.bak"] {
    assert_eq!(Language::for_file_name(name), None, "{name}");
  }
  assert_eq!(Language::from_name(Language::Dotenv.name()), Some(Language::Dotenv));
}
