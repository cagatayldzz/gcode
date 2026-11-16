use std::{fs, path::PathBuf};
use vekro_git::summary;

fn temp_dir(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("vekro-git-{name}-{}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  dir
}

#[test]
fn a_folder_outside_a_repository_has_no_summary() {
  let dir = temp_dir("plain");
  assert_eq!(summary(&dir), None);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_new_repository_names_its_branch_and_counts_untracked_files() {
  let dir = temp_dir("fresh");
  git2::Repository::init_opts(&dir, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  fs::write(dir.join("a.txt"), "a").unwrap();
  fs::write(dir.join("b.txt"), "b").unwrap();
  let found = summary(&dir).unwrap();
  assert_eq!((found.branch.as_str(), found.changed), ("main", 2));
  fs::remove_dir_all(dir).unwrap();
}
