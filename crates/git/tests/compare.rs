use std::{fs, path::PathBuf};
use vekro_git::{compare, compare_choices, merge_branch};

fn repository(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("vekro-git-compare-{name}-{}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  let repository = git2::Repository::init_opts(&dir, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  let mut config = repository.config().unwrap();
  config.set_str("user.name", "Ada").unwrap();
  config.set_str("user.email", "ada@example.com").unwrap();
  dir
}

fn commit_file(dir: &std::path::Path, file: &str, text: &str, message: &str, seconds: i64) {
  fs::write(dir.join(file), text).unwrap();
  let repository = git2::Repository::open(dir).unwrap();
  let mut index = repository.index().unwrap();
  index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
  index.write().unwrap();
  let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
  let signature = git2::Signature::new("Ada", "ada@example.com", &git2::Time::new(seconds, 0)).unwrap();
  let parent = repository.head().ok().and_then(|head| head.peel_to_commit().ok());
  repository.commit(Some("HEAD"), &signature, &signature, message, &tree, &parent.iter().collect::<Vec<_>>()).unwrap();
}

fn checkout(dir: &std::path::Path, branch: &str) {
  vekro_git::switch_branch(dir, branch).unwrap();
}

/// `main` has one commit more than `feature` had when it branched off; `feature` has two of its own.
fn diverged(name: &str) -> PathBuf {
  let dir = repository(name);
  commit_file(&dir, "a.txt", "one\n", "feat: base", 1_000);
  vekro_git::create_branch(&dir, "feature").unwrap();
  commit_file(&dir, "b.txt", "b\n", "feat: b", 2_000);
  commit_file(&dir, "c.txt", "c\n", "feat: c", 3_000);
  checkout(&dir, "main");
  commit_file(&dir, "d.txt", "d\n", "fix: d", 4_000);
  dir
}

#[test]
fn comparing_lists_what_each_side_has_that_the_other_lacks() {
  let dir = diverged("sides");
  let found = compare(&dir, "feature").unwrap();
  assert_eq!((found.behind_count, found.ahead_count), (2, 1));
  assert_eq!(found.behind.iter().map(|commit| commit.summary.as_str()).collect::<Vec<_>>(), ["feat: c", "feat: b"]);
  assert_eq!(found.ahead.iter().map(|commit| commit.summary.as_str()).collect::<Vec<_>>(), ["fix: d"]);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_choices_leave_out_the_current_branch_and_count_both_ways() {
  let dir = diverged("choices");
  let choices = compare_choices(&dir).unwrap();
  assert_eq!(choices.default, None, "main is the current branch, so only feature is left");
  // feature was checked out lately, so it is in the recent group.
  assert!(choices.other.is_empty());
  assert_eq!(choices.recent.len(), 1);
  let feature = &choices.recent[0];
  assert_eq!((feature.name.as_str(), feature.behind, feature.ahead), ("feature", 2, 1));
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn recently_used_branches_are_listed_newest_first_and_once() {
  let dir = diverged("recent");
  vekro_git::create_branch(&dir, "zzz").unwrap();
  checkout(&dir, "main");
  vekro_git::create_branch(&dir, "aaa").unwrap();
  checkout(&dir, "main");
  let choices = compare_choices(&dir).unwrap();
  assert_eq!(choices.recent.iter().map(|branch| branch.name.as_str()).collect::<Vec<_>>(), ["aaa", "zzz", "feature"], "newest checkout first");
  let all: Vec<String> = choices.default.iter().chain(&choices.recent).chain(&choices.other).map(|branch| branch.name.clone()).collect();
  assert!(["aaa", "feature", "zzz"].iter().all(|name| all.iter().any(|found| found == name)), "every branch is offered once: {all:?}");
  assert_eq!(all.len(), 3);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn merging_brings_the_other_branch_in() {
  let dir = diverged("merge");
  merge_branch(&dir, "feature").unwrap();
  assert!(dir.join("b.txt").exists() && dir.join("c.txt").exists() && dir.join("d.txt").exists());
  let after = compare(&dir, "feature").unwrap();
  assert_eq!(after.behind_count, 0);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_conflicting_merge_changes_nothing_and_says_which_files() {
  let dir = repository("conflict");
  commit_file(&dir, "a.txt", "one\n", "feat: base", 1_000);
  vekro_git::create_branch(&dir, "other").unwrap();
  commit_file(&dir, "a.txt", "other\n", "feat: other", 2_000);
  checkout(&dir, "main");
  commit_file(&dir, "a.txt", "main\n", "feat: main", 3_000);
  let error = merge_branch(&dir, "other").unwrap_err();
  assert!(error.contains("a.txt") && error.contains("Nothing was changed"), "{error}");
  assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "main\n");
  assert!(!dir.join(".git").join("MERGE_HEAD").exists());
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_name_that_looks_like_an_option_is_refused() {
  let dir = repository("option");
  commit_file(&dir, "a.txt", "one\n", "feat: base", 1_000);
  assert!(merge_branch(&dir, "--abort").is_err());
  fs::remove_dir_all(dir).unwrap();
}
