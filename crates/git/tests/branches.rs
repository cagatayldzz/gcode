use std::{fs, path::PathBuf};
use vekro_git::{branches, create_branch, diff_files, summary, switch_branch};

fn repository(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("vekro-git-branches-{name}-{}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  let repository = git2::Repository::init_opts(&dir, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  let mut config = repository.config().unwrap();
  config.set_str("user.name", "Test").unwrap();
  config.set_str("user.email", "test@example.com").unwrap();
  dir
}

fn commit_all(dir: &std::path::Path, message: &str) {
  let repository = git2::Repository::open(dir).unwrap();
  let mut index = repository.index().unwrap();
  index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
  index.write().unwrap();
  let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
  let signature = repository.signature().unwrap();
  let parent = repository.head().ok().and_then(|head| head.peel_to_commit().ok());
  repository.commit(Some("HEAD"), &signature, &signature, message, &tree, &parent.iter().collect::<Vec<_>>()).unwrap();
}

#[test]
fn a_new_branch_is_created_and_checked_out() {
  let dir = repository("create");
  fs::write(dir.join("a.txt"), "a").unwrap();
  commit_all(&dir, "first");
  create_branch(&dir, "feature/search").unwrap();
  assert_eq!(summary(&dir).unwrap().branch, "feature/search");
  assert_eq!(branches(&dir).unwrap(), vec!["feature/search", "main"]);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn bad_and_duplicate_branch_names_are_refused() {
  let dir = repository("names");
  fs::write(dir.join("a.txt"), "a").unwrap();
  commit_all(&dir, "first");
  assert!(create_branch(&dir, "  ").is_err());
  assert!(create_branch(&dir, "has space").is_err());
  assert!(create_branch(&dir, "main").unwrap_err().contains("already exists"));
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_branch_in_a_repository_without_commits_renames_the_unborn_one() {
  let dir = repository("unborn");
  create_branch(&dir, "develop").unwrap();
  assert_eq!(summary(&dir).unwrap().branch, "develop");
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn switching_moves_the_files_and_keeps_uncommitted_work() {
  let dir = repository("switch");
  fs::write(dir.join("a.txt"), "main").unwrap();
  commit_all(&dir, "first");
  create_branch(&dir, "other").unwrap();
  fs::write(dir.join("b.txt"), "only on other").unwrap();
  commit_all(&dir, "second");
  switch_branch(&dir, "main").unwrap();
  assert!(!dir.join("b.txt").exists());
  fs::write(dir.join("a.txt"), "edited").unwrap();
  switch_branch(&dir, "other").unwrap();
  assert!(dir.join("b.txt").exists());
  assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "edited");
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_switch_that_would_overwrite_changes_is_refused_and_loses_nothing() {
  let dir = repository("conflict");
  fs::write(dir.join("a.txt"), "one").unwrap();
  commit_all(&dir, "first");
  create_branch(&dir, "other").unwrap();
  fs::write(dir.join("a.txt"), "two").unwrap();
  commit_all(&dir, "second");
  switch_branch(&dir, "main").unwrap();
  fs::write(dir.join("a.txt"), "unsaved").unwrap();
  let error = switch_branch(&dir, "other").unwrap_err();
  assert!(error.contains("overwritten"), "{error}");
  assert_eq!(summary(&dir).unwrap().branch, "main");
  assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "unsaved");
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn switching_to_a_missing_branch_fails() {
  let dir = repository("missing");
  fs::write(dir.join("a.txt"), "a").unwrap();
  commit_all(&dir, "first");
  assert!(switch_branch(&dir, "nope").unwrap_err().contains("no branch"));
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_diff_of_several_files_names_each_of_them() {
  let dir = repository("diff");
  fs::write(dir.join("a.txt"), "a\n").unwrap();
  commit_all(&dir, "first");
  fs::write(dir.join("a.txt"), "changed\n").unwrap();
  fs::write(dir.join("new.txt"), "fresh\n").unwrap();
  let diff = diff_files(&dir, &[PathBuf::from("a.txt"), PathBuf::from("new.txt")]).unwrap();
  assert!(diff.contains("diff --git a/a.txt b/a.txt") && diff.contains("+changed"));
  assert!(diff.contains("diff --git a/new.txt b/new.txt") && diff.contains("+fresh"));
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pushing_publishes_a_branch_that_has_no_upstream_yet() {
  let remote = std::env::temp_dir().join(format!("vekro-git-branches-remote-{}", std::process::id()));
  let _ = fs::remove_dir_all(&remote);
  git2::Repository::init_bare(&remote).unwrap();
  let dir = repository("push");
  fs::write(dir.join("a.txt"), "a").unwrap();
  commit_all(&dir, "first");
  git2::Repository::open(&dir).unwrap().remote("origin", remote.to_str().unwrap()).unwrap();
  vekro_git::push(&dir).unwrap();
  assert!(git2::Repository::open_bare(&remote).unwrap().find_reference("refs/heads/main").is_ok());
  fs::write(dir.join("b.txt"), "b").unwrap();
  commit_all(&dir, "second");
  vekro_git::push(&dir).unwrap();
  fs::remove_dir_all(dir).unwrap();
  fs::remove_dir_all(remote).unwrap();
}

#[test]
fn pushing_without_a_remote_reports_git_s_message() {
  let dir = repository("noremote");
  fs::write(dir.join("a.txt"), "a").unwrap();
  commit_all(&dir, "first");
  assert!(!vekro_git::push(&dir).unwrap_err().is_empty());
  fs::remove_dir_all(dir).unwrap();
}
