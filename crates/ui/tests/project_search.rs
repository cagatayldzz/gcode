#[path = "../src/project_search.rs"]
mod project_search;

use std::{
  fs,
  path::PathBuf,
  sync::atomic::AtomicBool,
  time::{SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);

impl Fixture {
  fn new() -> Self {
    let path =
      std::env::temp_dir().join(format!("vekro-search-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&path).unwrap();
    Self(path)
  }
}

impl Drop for Fixture {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

#[test]
fn finds_text_with_locations_and_respects_ignore_rules() {
  let fixture = Fixture::new();
  fs::write(fixture.0.join(".gitignore"), "ignored.txt\n").unwrap();
  fs::write(fixture.0.join("source.rs"), "first\n😀 Café NEEDLE here\n").unwrap();
  fs::write(fixture.0.join("ignored.txt"), "needle").unwrap();
  fs::write(fixture.0.join("binary"), b"needle\0binary").unwrap();
  let options = project_search::Options { query: "needle".into(), ..Default::default() };
  let results = project_search::search(&fixture.0, &options, &AtomicBool::new(false)).unwrap().matches;
  assert_eq!(results.len(), 1);
  assert_eq!(results[0].path.file_name().unwrap(), "source.rs");
  assert_eq!((results[0].line, results[0].column), (1, 8));
  assert_eq!(results[0].preview, "😀 Café NEEDLE here");
  assert_eq!(&results[0].preview[results[0].highlight.clone()], "NEEDLE");
}

#[test]
fn search_applies_case_word_and_glob_filters_and_counts_each_match() {
  let fixture = Fixture::new();
  fs::create_dir(fixture.0.join("src")).unwrap();
  fs::write(fixture.0.join("src/main.rs"), "export EXPORT exports export\n").unwrap();
  fs::write(fixture.0.join("src/generated.rs"), "export\n").unwrap();
  fs::write(fixture.0.join("readme.txt"), "export\n").unwrap();
  let options = project_search::Options {
    query: "export".into(),
    case_sensitive: true,
    whole_word: true,
    include: "src/**".into(),
    exclude: "*.generated.rs, generated.rs".into(),
    ..Default::default()
  };
  let result = project_search::search(&fixture.0, &options, &AtomicBool::new(false)).unwrap();
  assert_eq!((result.occurrences, result.files), (2, 1));
  assert_eq!(result.matches.iter().map(|found| found.column).collect::<Vec<_>>(), [0, 22]);
}

#[test]
fn replacement_expands_regex_captures_preserves_line_endings_and_refuses_stale_edits() {
  let fixture = Fixture::new();
  let file = fixture.0.join("code.txt");
  fs::write(&file, "name: Ada\r\nname: Grace\r\n").unwrap();
  let options = project_search::Options { query: "name: (\\w+)".into(), regex: true, ..Default::default() };
  let plan = project_search::replacement_plan(&fixture.0, &options, "$1", false).unwrap();
  assert_eq!(plan.len(), 1);
  assert_eq!(plan[0].path, file);
  assert_eq!(plan[0].updated, "Ada\r\nGrace\r\n");
  project_search::apply_replacement(&fixture.0, &plan[0]).unwrap();
  assert_eq!(fs::read_to_string(&file).unwrap(), "Ada\r\nGrace\r\n");
  fs::write(&file, "External change\n").unwrap();
  assert!(project_search::apply_replacement(&fixture.0, &plan[0]).is_err());
  assert_eq!(fs::read_to_string(&file).unwrap(), "External change\n");
}
