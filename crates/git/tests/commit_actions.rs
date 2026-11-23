use std::{fs, path::Path, path::PathBuf, process::Command};

fn git(dir: &Path, arguments: &[&str]) -> String {
  let output = Command::new("git")
    .arg("-C")
    .arg(dir)
    .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false"])
    .args(arguments)
    .output()
    .unwrap();
  assert!(output.status.success(), "git {arguments:?}: {}", String::from_utf8_lossy(&output.stderr));
  String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn repository(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("ws-actions-{name}-{}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  git(&dir, &["init", "--initial-branch=main"]);
  // The library reads the identity from the repository: CI machines have none of their own.
  git(&dir, &["config", "user.name", "Test"]);
  git(&dir, &["config", "user.email", "test@example.com"]);
  fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
  git(&dir, &["add", "."]);
  git(&dir, &["commit", "-m", "first"]);
  dir
}

fn commit_file(dir: &Path, file: &str, text: &str, message: &str) -> String {
  fs::write(dir.join(file), text).unwrap();
  git(dir, &["add", "."]);
  git(dir, &["commit", "-m", message]);
  git(dir, &["rev-parse", "HEAD"])
}

#[test]
fn reverting_a_commit_adds_a_commit_that_undoes_it() {
  let dir = repository("revert");
  let added = commit_file(&dir, "b.txt", "b", "add b");

  let short = ws_git::revert_commit(&dir, &added).unwrap();

  assert!(!dir.join("b.txt").exists());
  assert_eq!(git(&dir, &["log", "-1", "--format=%s"]), "Revert \"add b\"");
  assert!(git(&dir, &["rev-parse", "HEAD"]).starts_with(&short));
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_conflicting_revert_changes_nothing() {
  let dir = repository("revert-conflict");
  let first = commit_file(&dir, "a.txt", "one\nTWO\nthree\n", "edit two");
  commit_file(&dir, "a.txt", "one\nOTHER\nthree\n", "edit two again");

  let error = ws_git::revert_commit(&dir, &first).unwrap_err();

  assert!(error.contains("conflicts in a.txt"), "{error}");
  assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one\nOTHER\nthree\n");
  assert!(!ws_git::summary(&dir).unwrap().merging);
  assert!(ws_git::summary(&dir).unwrap().changes.is_empty());
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reverting_with_uncommitted_changes_is_refused() {
  let dir = repository("revert-dirty");
  let added = commit_file(&dir, "b.txt", "b", "add b");
  fs::write(dir.join("a.txt"), "dirty").unwrap();

  assert!(ws_git::revert_commit(&dir, &added).unwrap_err().contains("before reverting"));
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_commit_is_cherry_picked_onto_another_branch() {
  let dir = repository("cherry");
  git(&dir, &["checkout", "-b", "release"]);
  git(&dir, &["checkout", "main"]);
  let picked = commit_file(&dir, "feature.txt", "feature", "add feature");

  ws_git::cherry_pick(&dir, &picked, "release").unwrap();

  assert_eq!(ws_git::summary(&dir).unwrap().branch, "release");
  assert!(dir.join("feature.txt").exists());
  assert_eq!(git(&dir, &["log", "-1", "--format=%s"]), "add feature");
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_conflicting_cherry_pick_returns_to_the_branch_it_started_on() {
  let dir = repository("cherry-conflict");
  git(&dir, &["checkout", "-b", "release"]);
  commit_file(&dir, "a.txt", "one\nRELEASE\nthree\n", "release edit");
  git(&dir, &["checkout", "main"]);
  let picked = commit_file(&dir, "a.txt", "one\nMAIN\nthree\n", "main edit");

  let error = ws_git::cherry_pick(&dir, &picked, "release").unwrap_err();

  assert!(error.contains("conflicts in a.txt"), "{error}");
  assert_eq!(ws_git::summary(&dir).unwrap().branch, "main");
  assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one\nMAIN\nthree\n");
  assert!(ws_git::summary(&dir).unwrap().changes.is_empty());
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_branch_is_created_at_an_older_commit() {
  let dir = repository("from-commit");
  let first = git(&dir, &["rev-parse", "HEAD"]);
  commit_file(&dir, "later.txt", "later", "later work");

  ws_git::branch_from_commit(&dir, &first, "hotfix").unwrap();

  assert_eq!(ws_git::summary(&dir).unwrap().branch, "hotfix");
  assert!(!dir.join("later.txt").exists());
  assert!(ws_git::branch_from_commit(&dir, &first, "hotfix").unwrap_err().contains("already exists"));
  assert!(ws_git::branch_from_commit(&dir, &first, "bad name").is_err());
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn amending_adds_files_and_replaces_the_message() {
  let dir = repository("amend");
  commit_file(&dir, "b.txt", "b", "add b");
  fs::write(dir.join("c.txt"), "c").unwrap();

  ws_git::amend_commit(&dir, &[PathBuf::from("c.txt")], "add b and c", "More detail.").unwrap();

  assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]), "2", "no extra commit");
  assert_eq!(git(&dir, &["log", "-1", "--format=%s"]), "add b and c");
  assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).lines().count(), 2);
  assert!(ws_git::summary(&dir).unwrap().changes.is_empty());
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn amending_only_the_message_keeps_the_files() {
  let dir = repository("amend-message");
  commit_file(&dir, "b.txt", "b", "typo mesage");

  ws_git::amend_commit(&dir, &[], "fixed message", "").unwrap();

  assert_eq!(git(&dir, &["log", "-1", "--format=%s"]), "fixed message");
  assert!(dir.join("b.txt").exists());
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_pushed_commit_cannot_be_amended() {
  let dir = repository("amend-pushed");
  let origin = dir.with_extension("origin.git");
  let _ = fs::remove_dir_all(&origin);
  git(&dir, &["init", "--bare", "--initial-branch=main", origin.to_str().unwrap()]);
  git(&dir, &["remote", "add", "origin", origin.to_str().unwrap()]);
  git(&dir, &["push", "-u", "origin", "main"]);

  let error = ws_git::amend_commit(&dir, &[], "rewrite", "").unwrap_err();

  assert!(error.contains("already on the remote"), "{error}");
  fs::remove_dir_all(dir).unwrap();
  fs::remove_dir_all(origin).unwrap();
}

#[test]
fn ignore_rules_are_appended_once_to_the_gitignore() {
  let dir = repository("ignore");
  fs::write(dir.join(".gitignore"), "target").unwrap();

  ws_git::add_to_gitignore(&dir, &["/secret.txt".into(), "*.log".into(), "target".into(), "*.log".into()]).unwrap();

  assert_eq!(fs::read_to_string(dir.join(".gitignore")).unwrap(), "target\n/secret.txt\n*.log\n");
  fs::write(dir.join("secret.txt"), "x").unwrap();
  fs::write(dir.join("debug.log"), "x").unwrap();
  assert!(ws_git::summary(&dir)
    .unwrap()
    .changes
    .iter()
    .all(|change| change.path != Path::new("secret.txt") && change.path != Path::new("debug.log")));
  assert!(ws_git::add_to_gitignore(&dir, &["two\nlines".into()]).is_err());
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn recent_authors_lists_other_people_once_and_leaves_out_the_committer() {
  let dir = repository("authors");
  fs::write(dir.join("b.txt"), "b").unwrap();
  git(&dir, &["add", "."]);
  git(&dir, &["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "commit", "-m", "by ada", "--author=Ada <ada@example.com>"]);
  fs::write(dir.join("c.txt"), "c").unwrap();
  git(&dir, &["add", "."]);
  git(&dir, &["commit", "-m", "by ada again", "--author=Ada <ADA@example.com>"]);

  assert_eq!(ws_git::recent_authors(&dir, 100, 10), vec![("Ada".to_string(), "ADA@example.com".to_string())]);
  fs::remove_dir_all(dir).unwrap();
}
