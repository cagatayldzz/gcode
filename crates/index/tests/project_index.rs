use std::{fs, path::Path, thread, time::Duration};
use vekro_index::ProjectIndex;

fn write(root: &Path, path: &str, text: &str) {
  let file = root.join(path);
  fs::create_dir_all(file.parent().unwrap()).unwrap();
  fs::write(file, text).unwrap();
}

fn names(index: &ProjectIndex) -> Vec<String> {
  index.files().iter().map(|file| file.path.display().to_string()).collect()
}

/// Folder times of change have a granularity; waits until a new change is certain to show.
fn tick() {
  thread::sleep(Duration::from_millis(20));
}

#[test]
fn the_index_lists_the_files_the_ignore_rules_let_through() {
  let project = tempfile::tempdir().unwrap();
  write(project.path(), "src/main.rs", "fn main() {}");
  write(project.path(), "README.md", "hello");
  write(project.path(), ".gitignore", "target/\n*.log\n");
  write(project.path(), "target/debug/app", "binary");
  write(project.path(), "run.log", "log");
  write(project.path(), ".git/config", "[core]");
  let index = ProjectIndex::scan(project.path());
  assert_eq!(names(&index), vec![".gitignore", "README.md", "src/main.rs"]);
  assert_eq!(index.files().iter().find(|file| file.path == Path::new("README.md")).unwrap().size, 5);
}

#[test]
fn a_saved_index_is_used_again_while_nothing_changed() {
  let project = tempfile::tempdir().unwrap();
  let cache = tempfile::tempdir().unwrap();
  write(project.path(), "a.txt", "a");
  write(project.path(), "src/b.txt", "b");
  let first = ProjectIndex::open(cache.path(), project.path());
  assert!(!first.from_cache, "the first time the tree is walked");
  let second = ProjectIndex::open(cache.path(), project.path());
  assert!(second.from_cache, "the second time the saved index is enough");
  assert_eq!(second.index, first.index);
}

#[test]
fn a_new_or_deleted_file_makes_the_saved_index_stale() {
  let project = tempfile::tempdir().unwrap();
  let cache = tempfile::tempdir().unwrap();
  write(project.path(), "src/a.txt", "a");
  ProjectIndex::open(cache.path(), project.path());
  tick();
  write(project.path(), "src/new.txt", "n");
  let after_create = ProjectIndex::open(cache.path(), project.path());
  assert!(!after_create.from_cache);
  assert_eq!(names(&after_create.index), vec!["src/a.txt", "src/new.txt"]);
  tick();
  fs::remove_file(project.path().join("src/a.txt")).unwrap();
  let after_delete = ProjectIndex::open(cache.path(), project.path());
  assert!(!after_delete.from_cache);
  assert_eq!(names(&after_delete.index), vec!["src/new.txt"]);
}

#[test]
fn a_changed_ignore_file_makes_the_saved_index_stale() {
  let project = tempfile::tempdir().unwrap();
  let cache = tempfile::tempdir().unwrap();
  write(project.path(), "keep.txt", "k");
  write(project.path(), "drop.txt", "d");
  write(project.path(), "pkg/.gitignore", "");
  ProjectIndex::open(cache.path(), project.path());
  tick();
  // An edit in place does not move the folder's time, only the ignore file's.
  write(project.path(), ".gitignore", "drop.txt\n");
  let reopened = ProjectIndex::open(cache.path(), project.path());
  assert!(!reopened.from_cache);
  assert_eq!(names(&reopened.index), vec![".gitignore", "keep.txt", "pkg/.gitignore"]);
}

#[test]
fn a_damaged_or_foreign_saved_index_is_ignored() {
  let project = tempfile::tempdir().unwrap();
  let other = tempfile::tempdir().unwrap();
  let cache = tempfile::tempdir().unwrap();
  write(project.path(), "a.txt", "a");
  let index = ProjectIndex::scan(project.path());
  index.save(cache.path()).unwrap();
  assert_eq!(ProjectIndex::load(cache.path(), project.path()), Some(index));
  assert!(ProjectIndex::load(cache.path(), other.path()).is_none(), "another project has no index yet");
  let file = ProjectIndex::cache_file(cache.path(), project.path());
  let bytes = fs::read(&file).unwrap();
  fs::write(&file, &bytes[..bytes.len() / 2]).unwrap();
  assert!(ProjectIndex::load(cache.path(), project.path()).is_none(), "a cut file is not trusted");
  fs::write(&file, b"garbage").unwrap();
  assert!(ProjectIndex::load(cache.path(), project.path()).is_none());
  assert_eq!(names(&ProjectIndex::open(cache.path(), project.path()).index), vec!["a.txt"], "opening still works and repairs it");
  assert!(ProjectIndex::load(cache.path(), project.path()).is_some());
}

#[test]
fn paths_join_the_project_folder() {
  let project = tempfile::tempdir().unwrap();
  write(project.path(), "src/a.rs", "");
  let index = ProjectIndex::scan(project.path());
  assert_eq!(index.paths().collect::<Vec<_>>(), vec![project.path().join("src/a.rs")]);
}

#[test]
fn saving_from_several_threads_at_once_leaves_one_good_index_and_no_stray_files() {
  let project = tempfile::tempdir().unwrap();
  let cache = tempfile::tempdir().unwrap();
  for number in 0..50 {
    write(project.path(), &format!("src/file{number}.rs"), "x");
  }
  let index = ProjectIndex::scan(project.path());
  thread::scope(|scope| {
    for _ in 0..8 {
      scope.spawn(|| {
        for _ in 0..20 {
          index.save(cache.path()).unwrap();
          assert!(ProjectIndex::open(cache.path(), project.path()).index.files().len() == 50);
        }
      });
    }
  });
  assert_eq!(ProjectIndex::load(cache.path(), project.path()), Some(index));
  let left: Vec<_> = fs::read_dir(cache.path()).unwrap().map(|entry| entry.unwrap().file_name()).collect();
  assert_eq!(left.len(), 1, "only the index itself is left: {left:?}");
}
