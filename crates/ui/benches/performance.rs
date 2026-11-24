//! Manual benchmarks of the production paths used by the editor and Explorer.

#[allow(dead_code)]
#[path = "../src/explorer.rs"]
mod explorer;
#[path = "../src/minimap/outline.rs"]
mod outline;
#[path = "../src/project_search.rs"]
mod project_search;

use gpui_kit::{base::input::Rope, component::highlighter::HighlightTheme};
use std::{
  collections::HashSet,
  fs,
  hint::black_box,
  path::{Path, PathBuf},
  sync::atomic::AtomicBool,
  time::Instant,
};
use ws_index::highlight::{Highlights, Language};
use ws_storage::Preferences;

const FOLDERS: usize = 16;
const FILES_PER_FOLDER: usize = 32;
const HISTORY_LENGTH: usize = 128;
const LARGE_FILE: &str = "large.rs";
const SOURCE: &str = "    let benchmark_marker = value.map(|value| value + 1);\n";
const LARGE_FILE_LINES: usize = 24_000;

struct Run {
  samples: usize,
  filter: Option<String>,
  cases: usize,
}

impl Run {
  fn measure<T>(&mut self, name: &str, mut prepare: impl FnMut(), mut operation: impl FnMut() -> T) {
    if self.filter.as_ref().is_some_and(|filter| !name.contains(filter)) {
      return;
    }
    self.cases += 1;
    if self.samples == 0 {
      prepare();
      drop(black_box(operation()));
      println!("Verified {name}");
      return;
    }
    for _ in 0..3 {
      prepare();
      drop(black_box(operation()));
    }
    let mut samples = Vec::with_capacity(self.samples);
    for _ in 0..self.samples {
      prepare();
      let start = Instant::now();
      drop(black_box(operation()));
      samples.push(start.elapsed().as_secs_f64() * 1_000_000.);
    }
    samples.sort_by(f64::total_cmp);
    let middle = samples.len() / 2;
    let median = (samples[(samples.len() - 1) / 2] + samples[middle]) / 2.;
    let p95 = samples[(samples.len() * 95).div_ceil(100) - 1];
    println!("{name},{},{median:.3},{p95:.3}", samples.len());
  }
}

fn create_project(root: &Path) -> HashSet<PathBuf> {
  let mut expanded = HashSet::new();
  let source = SOURCE.repeat(80);
  for folder in 0..FOLDERS {
    let dir = root.join(format!("module_{folder:02}"));
    fs::create_dir_all(&dir).unwrap();
    for file in 0..FILES_PER_FOLDER {
      fs::write(dir.join(format!("file_{file:02}.rs")), &source).unwrap();
    }
    fs::write(dir.join("notes.md"), "Fixture for the benchmarks.\n").unwrap();
    expanded.insert(dir);
  }
  let mut large = String::new();
  for _ in 0..LARGE_FILE_LINES / 8 {
    large.push_str(concat!(
      "fn process(value: Option<u32>) -> u32 {\n",
      "    let benchmark_marker = value.map(|value| value + 1);\n",
      "    match benchmark_marker {\n",
      "        Some(result) => result,\n",
      "        None => 0,\n",
      "    }\n",
      "}\n\n",
    ));
  }
  fs::write(root.join(LARGE_FILE), large).unwrap();
  fs::write(root.join(".gitignore"), "/scratch/\n").unwrap();
  expanded
}

fn create_history(root: &Path) {
  let repository = git2::Repository::init_opts(root, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  let signature = git2::Signature::now("Benchmark", "benchmark@example.test").unwrap();
  let mut index = repository.index().unwrap();
  let timeline = Path::new("timeline.rs");
  let noise = Path::new("unrelated.rs");
  for number in 0..HISTORY_LENGTH {
    fs::write(root.join(noise), format!("// Commit {number}\n")).unwrap();
    index.add_path(noise).unwrap();
    if number % 8 == 0 {
      fs::write(root.join(timeline), format!("// Timeline version {number}\n")).unwrap();
      index.add_path(timeline).unwrap();
    }
    let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repository.head().ok().and_then(|head| head.peel_to_commit().ok());
    repository.commit(Some("HEAD"), &signature, &signature, "Fixture commit", &tree, &parent.iter().collect::<Vec<_>>()).unwrap();
  }
  index.write().unwrap();
  fs::write(root.join(timeline), "// Unsaved in Git\n").unwrap();
}

fn benchmark(run: &mut Run, root: &Path, expanded: &HashSet<PathBuf>) {
  let scratch = root.join("scratch");
  fs::create_dir(&scratch).unwrap();
  let source = root.join(LARGE_FILE);
  let text = fs::read_to_string(&source).unwrap();
  let rope = Rope::from_str(&text);
  let short_rope = Rope::from_str(&SOURCE.repeat(80));
  let unexpanded = HashSet::new();
  let literal = project_search::Options { query: "benchmark_marker".to_owned(), ..Default::default() };
  let regex =
    project_search::Options { query: r"benchmark_(marker)\s*=".to_owned(), regex: true, include: "module_00".to_owned(), ..Default::default() };
  let cancelled = AtomicBool::new(false);
  let replace = project_search::Options { include: "large.rs".to_owned(), ..literal.clone() };
  let mut plan = project_search::replacement_plan(root, &replace, "replacement_marker", false, None).unwrap();
  let edit = plan.pop().expect("large file has replacements");
  let copy = scratch.join("copy.rs");
  let metadata = fs::symlink_metadata(&source).unwrap();
  let preferences = Preferences { explorer_font_size: 16, ..Preferences::default() };
  let config = scratch.join("preferences.json");

  let syntax_theme = HighlightTheme::default_light();
  let sample = |text| outline::sample(text, "rust", &syntax_theme);
  assert_eq!(sample(&rope).len(), 640);
  assert!(sample(&short_rope).iter().any(|line| line.text.contains(' ') && !line.colors.is_empty()));
  assert_eq!(explorer::rows(root, expanded).iter().filter(|row| row.depth == 1).count(), FOLDERS * (FILES_PER_FOLDER + 1));
  let found = project_search::search(root, &regex, &cancelled, None).unwrap();
  assert_eq!(found.files, FILES_PER_FOLDER);
  let first = &found.matches[0];
  assert!(first.path.starts_with(root.join("module_00")));
  assert_eq!((first.line, first.column), (0, 8));
  assert!(first.preview[first.highlight.clone()].starts_with("benchmark_marker"));
  assert_eq!(ws_git::file_history(&root.join("timeline.rs"), 100).unwrap().len(), HISTORY_LENGTH / 8);

  run.measure("explorer/collapsed", || {}, || explorer::rows(black_box(root), &unexpanded));
  run.measure("explorer/expanded", || {}, || explorer::rows(black_box(root), expanded));
  run.measure("search/literal", || {}, || project_search::search(root, &literal, &cancelled, None).unwrap());
  run.measure("search/regex_filtered", || {}, || project_search::search(root, &regex, &cancelled, None).unwrap());
  run.measure("search/replacement_plan", || {}, || project_search::replacement_plan(root, &replace, "replacement_marker", false, None).unwrap());
  run.measure("search/replacement_write", || fs::write(&source, &text).unwrap(), || project_search::apply_replacement(root, &edit).unwrap());
  fs::write(&source, &text).unwrap();
  run.measure("minimap/short_buffer", || {}, || sample(black_box(&short_rope)));
  // A `.env` file of 8k settings and 8k comments (under the size limit): classified once when the file is read, then only sliced for each screenful that is drawn.
  let env_text: String =
    (0..LARGE_FILE_LINES / 3).map(|line| format!("NEXT_PUBLIC_KEY_{line}=value/{line}+abc==\n# https://example.com/{line}\n")).collect();
  let env_highlights = Highlights::compute(Language::Dotenv, &env_text);
  let screenful = env_text.len() / 2..env_text.len() / 2 + 3_000;
  run.measure("highlight/dotenv_classify_16k_lines", || {}, || Highlights::compute(Language::Dotenv, black_box(&env_text)).runs().len());
  run.measure(
    "highlight/dotenv_draw_one_screenful",
    || {},
    || outline::line_styles(Language::Dotenv, black_box(&env_highlights), &screenful, &*syntax_theme),
  );
  run.measure("minimap/24k_lines", || {}, || sample(black_box(&rope)));
  run.measure("git/working_tree_summary", || {}, || ws_git::summary(root).unwrap());
  run.measure("git/file_history", || {}, || ws_git::file_history(&root.join("timeline.rs"), 100).unwrap());
  run.measure("git/file_diff", || {}, || ws_git::file_diff(root, Path::new("timeline.rs")).unwrap());
  run.measure(
    "files/copy_large_file",
    || {
      if copy.exists() {
        fs::remove_file(&copy).unwrap();
      }
    },
    || explorer::copy_file(&source, &copy, &metadata, false).unwrap(),
  );
  let renamed = scratch.join("renamed.rs");
  run.measure(
    "files/rename",
    || {
      if renamed.exists() {
        explorer::move_file(&renamed, &copy).unwrap();
      } else {
        fs::write(&copy, "File to rename.\n").unwrap();
      }
    },
    || explorer::move_file(&copy, &renamed).unwrap(),
  );
  run.measure(
    "settings/save_and_load",
    || {},
    || {
      preferences.save(&config).unwrap();
      Preferences::load(&config)
    },
  );
}

fn main() {
  let mut args = std::env::args().skip(1).filter(|arg| arg != "--bench");
  let first = args.next();
  if first.as_deref() == Some("--fixture") {
    assert!(args.next().is_none(), "--fixture does not take a path");
    let fixture = tempfile::Builder::new().prefix("bench-").tempdir().unwrap();
    let project = fixture.path().join("project");
    create_project(&project);
    create_history(&project);
    let other = fixture.path().join("other-project");
    fs::create_dir(&other).unwrap();
    fs::write(other.join("notes.md"), "# Second benchmark project\n").unwrap();
    println!("{}", fixture.keep().display());
    return;
  }
  let verify = matches!(first.as_deref(), Some("--verify" | "--test"));
  let filter = if verify { args.next() } else { first };
  assert!(args.next().is_none(), "Expected a case name filter, --verify, or --fixture");
  let samples = if verify {
    0
  } else {
    let value = std::env::var(ws_product::env_name("BENCH_SAMPLES")).unwrap_or_else(|_| "20".to_owned());
    value.parse::<usize>().ok().filter(|count| (1..=1000).contains(count)).expect("<prefix>_BENCH_SAMPLES must be between 1 and 1000")
  };
  let fixture = tempfile::tempdir().unwrap();
  let expanded = create_project(fixture.path());
  create_history(fixture.path());
  if !verify {
    println!("case,samples,median_us,p95_us");
  }
  let mut run = Run { samples, filter, cases: 0 };
  benchmark(&mut run, fixture.path(), &expanded);
  assert!(run.cases > 0, "No benchmark cases matched the filter");
}
