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

#[test]
fn repository_names_come_from_the_last_url_segment() {
  use vekro_git::repository_name;
  assert_eq!(repository_name("https://github.com/cagatayldzz/vekro.git"), Some("vekro".into()));
  assert_eq!(repository_name("git@github.com:cagatayldzz/vekro.git"), Some("vekro".into()));
  assert_eq!(repository_name(" https://github.com/cagatayldzz/vekro/ "), Some("vekro".into()));
  assert_eq!(repository_name(""), None);
}

#[test]
fn a_created_project_is_a_repository_on_main_and_cannot_be_created_twice() {
  let parent = temp_dir("create");
  let path = vekro_git::create_project(&parent, "demo").unwrap();
  assert_eq!(summary(&path).unwrap().branch, "main");
  assert!(vekro_git::create_project(&parent, "demo").is_err());
  assert!(vekro_git::create_project(&parent, "a/b").is_err());
  fs::remove_dir_all(parent).unwrap();
}

#[test]
fn cloning_copies_a_repository_and_reports_a_missing_one() {
  let parent = temp_dir("clone");
  let source = vekro_git::create_project(&parent, "source").unwrap();
  let destination = parent.join("elsewhere");
  fs::create_dir_all(&destination).unwrap();
  let cloned = vekro_git::clone(source.to_str().unwrap(), &destination).unwrap();
  assert_eq!(cloned, destination.join("source"));
  assert!(vekro_git::clone(parent.join("missing").to_str().unwrap(), &destination).is_err());
  fs::remove_dir_all(parent).unwrap();
}
