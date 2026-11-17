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
  let results = project_search::search(&fixture.0, "needle", &AtomicBool::new(false)).unwrap();
  assert_eq!(results.len(), 1);
  assert_eq!(results[0].path.file_name().unwrap(), "source.rs");
  assert_eq!((results[0].line, results[0].column), (1, 8));
  assert_eq!(results[0].preview, "😀 Café NEEDLE here");
  assert_eq!(&results[0].preview[results[0].highlight.clone()], "NEEDLE");
}
