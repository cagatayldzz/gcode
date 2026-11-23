use std::ops::Range;
use ws_index::highlight::{Highlights, Language, Role, Run};

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
  assert_eq!(role_at(&runs, ENV, "abc/def+ghi=="), Some(Role::Value), "a value is a string");
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
  assert_eq!(role_at(&runs, text, "/build"), None, "a slash is plain text");
  assert_eq!(role_at(&runs, text, "build"), Some(Role::Entity));
  assert_eq!(role_at(&runs, text, "#not-a-comment"), Some(Role::Entity));
  assert_eq!(role_at(&runs, text, "# indented"), Some(Role::Entity), "an indented hash is a pattern");
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
  let text = "KEY=value\n".repeat(ws_index::highlight::MAX_BYTES / 10 + 10);
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

const DOCKERFILE: &str = "# syntax=docker/dockerfile:1\nARG BUN_IMAGE=oven/bun:1.4.4-alpine\n\nFROM ${BUN_IMAGE} AS base\nWORKDIR /app\nENV NEXT_TELEMETRY_DISABLED=1\n\nfrom base as builder\nCOPY --from=builder /app/public ./public\nENV NODE_ENV=production \\\n    NEXT_TELEMETRY_DISABLED=1 \\\n    # a comment inside\n    PORT=3000 HOME=$HOME_DIR\nONBUILD RUN echo \"$USER\" \\$ESCAPED\nCMD [\"bun\", \"server.js\"]";

#[test]
fn dockerfile_instructions_stages_and_variables_have_their_roles() {
  let runs = Highlights::compute(Language::Dockerfile, DOCKERFILE).runs().to_vec();
  covers(&runs, &(0..DOCKERFILE.len()));
  assert_eq!(role_at(&runs, DOCKERFILE, "# syntax"), Some(Role::Comment));
  for (needle, expected) in [
    ("ARG", "ARG"),
    ("FROM ${", "FROM"),
    ("WORKDIR", "WORKDIR"),
    ("ENV NEXT", "ENV"),
    ("COPY", "COPY"),
    ("CMD", "CMD"),
    ("ONBUILD", "ONBUILD"),
    ("RUN echo", "RUN"),
  ] {
    assert_eq!(role_at(&runs, DOCKERFILE, needle), Some(Role::Instruction), "{needle}");
    assert_eq!(text_of(&runs, DOCKERFILE, needle), expected);
  }
  assert_eq!(role_at(&runs, DOCKERFILE, "from base"), Some(Role::Instruction), "instructions are not case sensitive");
  assert_eq!(role_at(&runs, DOCKERFILE, "AS base"), Some(Role::Instruction));
  assert_eq!(role_at(&runs, DOCKERFILE, "as builder"), Some(Role::Instruction));
  assert_eq!(text_of(&runs, DOCKERFILE, "${BUN_IMAGE}"), "${BUN_IMAGE}");
  assert_eq!(role_at(&runs, DOCKERFILE, "${BUN_IMAGE}"), Some(Role::Variable));
  assert_eq!(text_of(&runs, DOCKERFILE, "$HOME_DIR"), "$HOME_DIR");
  assert_eq!(role_at(&runs, DOCKERFILE, "$USER"), Some(Role::Value), "a variable inside quotes is part of the string");
  assert_eq!(role_at(&runs, DOCKERFILE, "$ESCAPED"), None, "an escaped dollar sign is plain text");
  assert_eq!(role_at(&runs, DOCKERFILE, "--from=builder"), None, "flags and values are plain text");
  assert_eq!(role_at(&runs, DOCKERFILE, "oven/bun"), None);
  assert_eq!(role_at(&runs, DOCKERFILE, "base\nWORKDIR"), None, "`base` after FROM is a name, not AS");
}

#[test]
fn a_continued_dockerfile_line_does_not_start_a_new_instruction() {
  let text = "ENV A=1 \\\n    RUN=2 \\\n    # note\n    COPY=3\nRUN ls\n";
  let runs = Highlights::compute(Language::Dockerfile, text).runs().to_vec();
  assert_eq!(role_at(&runs, text, "ENV"), Some(Role::Instruction));
  assert_eq!(role_at(&runs, text, "RUN=2"), None, "the rest of ENV is not an instruction even when it looks like one");
  assert_eq!(role_at(&runs, text, "COPY=3"), None);
  assert_eq!(role_at(&runs, text, "# note"), Some(Role::Comment), "a comment inside the continuation");
  assert_eq!(role_at(&runs, text, "RUN ls"), Some(Role::Instruction), "after the last line of the instruction a new one starts");
}

#[test]
fn a_word_that_only_starts_like_an_instruction_is_plain() {
  let text = "RUNNER x\nFROMAGE y\nCOPYRIGHT\n";
  let runs = Highlights::compute(Language::Dockerfile, text).runs().to_vec();
  assert_eq!(runs, vec![(0..text.len(), None)]);
}

#[test]
fn dockerfiles_are_found_by_file_name() {
  for name in ["Dockerfile", "dockerfile", "Dockerfile.dev", "Dockerfile.prod", "web.Dockerfile", "Containerfile"] {
    assert_eq!(Language::for_file_name(name), Some(Language::Dockerfile), "{name}");
  }
  for name in ["docker-compose.yml", ".dockerignore", "Dockerfiles", "dockerfile-notes.md", "my-Dockerfile"] {
    assert_eq!(Language::for_file_name(name), None, "{name}");
  }
  assert_eq!(Language::from_name(Language::Dockerfile.name()), Some(Language::Dockerfile));
}

#[test]
fn dockerfile_runs_cover_any_text_even_without_a_final_newline_or_with_carriage_returns() {
  for text in ["", "FROM x", "FROM x\r\nRUN y \\\r\n  z\r\n", "\\", "$", "${", "${A", "$$A", "FROM \\\n"] {
    covers(Highlights::compute(Language::Dockerfile, text).runs(), &(0..text.len()));
  }
}

#[test]
fn a_gitignore_pattern_marks_wildcards_and_negation_as_keywords_and_names_as_entities() {
  let text = "!keep.log\n*.log\nnode_modules/\n";
  let runs = Highlights::compute(Language::Gitignore, text).runs().to_vec();
  assert_eq!(role_at(&runs, text, "!keep"), Some(Role::Keyword));
  assert_eq!(role_at(&runs, text, "keep.log"), Some(Role::Entity));
  assert_eq!(role_at(&runs, text, "*.log"), Some(Role::Keyword));
  assert_eq!(role_at(&runs, text, ".log\nnode"), Some(Role::Entity));
  assert_eq!(role_at(&runs, text, "node_modules"), Some(Role::Entity));
}

#[test]
fn a_dotenv_equals_sign_is_a_keyword_and_a_quoted_value_is_one_string() {
  let text = "DATABASE_URL=\"postgres://user:pass@localhost:5432/db\"\n";
  let runs = Highlights::compute(Language::Dotenv, text).runs().to_vec();
  assert_eq!(role_at(&runs, text, "=\""), Some(Role::Keyword));
  assert_eq!(role_at(&runs, text, "\"postgres"), Some(Role::Value));
  assert_eq!(role_at(&runs, text, "5432"), Some(Role::Value));
}

#[test]
fn a_dockerfile_quoted_string_is_a_value() {
  let text = "CMD [\"node\", 'dist/index.js']\nRUN echo \"unclosed\n";
  let runs = Highlights::compute(Language::Dockerfile, text).runs().to_vec();
  assert_eq!(text_of(&runs, text, "\"node\""), "\"node\"");
  assert_eq!(role_at(&runs, text, "\"node\""), Some(Role::Value));
  assert_eq!(role_at(&runs, text, "'dist"), Some(Role::Value));
  assert_eq!(role_at(&runs, text, "\"unclosed"), None);
}
