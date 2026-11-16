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
  assert_eq!((found.branch.as_str(), found.changed()), ("main", 2));
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

#[test]
fn changes_list_new_modified_and_deleted_files_with_a_diff_for_each() {
  use vekro_git::{file_diff, ChangeKind};
  let dir = temp_dir("changes");
  let repository = git2::Repository::init_opts(&dir, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  fs::write(dir.join("kept.txt"), "one\n").unwrap();
  fs::write(dir.join("gone.txt"), "bye\n").unwrap();
  let mut index = repository.index().unwrap();
  index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
  index.write().unwrap();
  let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
  let signature = git2::Signature::now("Test", "test@example.com").unwrap();
  repository.commit(Some("HEAD"), &signature, &signature, "first", &tree, &[]).unwrap();

  fs::write(dir.join("kept.txt"), "one\ntwo\n").unwrap();
  fs::remove_file(dir.join("gone.txt")).unwrap();
  fs::write(dir.join("fresh.txt"), "new\n").unwrap();

  let found = summary(&dir).unwrap();
  let kinds: Vec<_> = found.changes.iter().map(|change| (change.path.to_str().unwrap().to_string(), change.kind)).collect();
  assert_eq!(
    kinds,
    [("fresh.txt".to_string(), ChangeKind::New), ("gone.txt".to_string(), ChangeKind::Deleted), ("kept.txt".to_string(), ChangeKind::Modified)]
  );
  let patch = file_diff(&dir, std::path::Path::new("kept.txt")).unwrap();
  assert!(patch.contains("+two"), "{patch}");
  assert!(file_diff(&dir, std::path::Path::new("fresh.txt")).unwrap().contains("+new"));
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn repositories_directly_inside_a_folder_are_found_by_name() {
  let parent = temp_dir("find");
  vekro_git::create_project(&parent, "beta").unwrap();
  vekro_git::create_project(&parent, "Alpha").unwrap();
  fs::create_dir_all(parent.join("plain-folder")).unwrap();
  fs::write(parent.join("a-file.txt"), "x").unwrap();
  let found: Vec<_> = vekro_git::find_repositories(&parent).iter().map(|path| path.file_name().unwrap().to_str().unwrap().to_string()).collect();
  assert_eq!(found, ["Alpha", "beta"]);
  assert!(vekro_git::find_repositories(&parent.join("missing")).is_empty());
  fs::remove_dir_all(parent).unwrap();
}

fn committed_repository(name: &str) -> PathBuf {
  let dir = temp_dir(name);
  let repository = git2::Repository::init_opts(&dir, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  fs::write(dir.join("kept.txt"), "original").unwrap();
  fs::write(dir.join("other.txt"), "other").unwrap();
  let mut index = repository.index().unwrap();
  index.add_path(std::path::Path::new("kept.txt")).unwrap();
  index.add_path(std::path::Path::new("other.txt")).unwrap();
  index.write().unwrap();
  let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
  let signature = git2::Signature::now("Test", "test@example.com").unwrap();
  repository.commit(Some("HEAD"), &signature, &signature, "first", &tree, &[]).unwrap();
  dir
}

#[test]
fn discarding_restores_a_modified_or_deleted_file_and_leaves_the_others_alone() {
  use vekro_git::discard_changes;
  let dir = committed_repository("discard-tracked");
  fs::write(dir.join("kept.txt"), "edited").unwrap();
  fs::write(dir.join("other.txt"), "edited too").unwrap();
  discard_changes(&dir, std::path::Path::new("kept.txt")).unwrap();
  assert_eq!(fs::read_to_string(dir.join("kept.txt")).unwrap(), "original");
  assert_eq!(fs::read_to_string(dir.join("other.txt")).unwrap(), "edited too");

  fs::remove_file(dir.join("other.txt")).unwrap();
  discard_changes(&dir, std::path::Path::new("other.txt")).unwrap();
  assert_eq!(fs::read_to_string(dir.join("other.txt")).unwrap(), "other");
  assert_eq!(summary(&dir).unwrap().changed(), 0);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn discarding_a_new_file_deletes_it_even_when_it_was_staged() {
  use vekro_git::discard_changes;
  let dir = committed_repository("discard-new");
  fs::write(dir.join("fresh.txt"), "new").unwrap();
  fs::write(dir.join("staged.txt"), "new").unwrap();
  let repository = git2::Repository::open(&dir).unwrap();
  let mut index = repository.index().unwrap();
  index.add_path(std::path::Path::new("staged.txt")).unwrap();
  index.write().unwrap();

  discard_changes(&dir, std::path::Path::new("fresh.txt")).unwrap();
  discard_changes(&dir, std::path::Path::new("staged.txt")).unwrap();
  assert!(!dir.join("fresh.txt").exists());
  assert!(!dir.join("staged.txt").exists());
  assert_eq!(summary(&dir).unwrap().changed(), 0);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_changed_file_is_found_by_its_repository_relative_path() {
  let dir = committed_repository("workdir-file");
  let found = vekro_git::workdir_file(&dir, std::path::Path::new("kept.txt")).unwrap();
  assert!(found.ends_with("kept.txt") && found.exists());
  fs::remove_dir_all(dir).unwrap();
}
