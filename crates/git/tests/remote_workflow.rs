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

/// A bare "origin" and two clones of it (`mine` is the project, `other` is a teammate), all in one temporary folder.
struct Remote {
  root: PathBuf,
  mine: PathBuf,
  other: PathBuf,
}

impl Remote {
  fn new(name: &str) -> Self {
    let root = std::env::temp_dir().join(format!("vekro-remote-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "--bare", "--initial-branch=main", "origin.git"]);
    let origin = root.join("origin.git");
    let seed = root.join("seed");
    git(&root, &["clone", origin.to_str().unwrap(), "seed"]);
    git(&seed, &["checkout", "-b", "main"]);
    fs::write(seed.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "first"]);
    git(&seed, &["push", "-u", "origin", "main"]);
    git(&root, &["clone", origin.to_str().unwrap(), "mine"]);
    git(&root, &["clone", origin.to_str().unwrap(), "other"]);
    Self { mine: root.join("mine"), other: root.join("other"), root }
  }
}

impl Drop for Remote {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.root);
  }
}

fn commit_file(dir: &Path, file: &str, text: &str, message: &str) {
  fs::write(dir.join(file), text).unwrap();
  git(dir, &["add", "."]);
  git(dir, &["commit", "-m", message]);
}

#[test]
fn pull_fast_forwards_when_only_the_remote_has_new_commits() {
  let remote = Remote::new("fast-forward");
  commit_file(&remote.other, "b.txt", "b", "teammate");
  git(&remote.other, &["push"]);
  vekro_git::fetch(&remote.mine, None, |_| {}).unwrap();
  let sync = vekro_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert_eq!((sync.ahead, sync.behind), (0, 1));
  vekro_git::pull(&remote.mine, None, |_| {}).unwrap();
  assert!(remote.mine.join("b.txt").exists());
  let sync = vekro_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert_eq!((sync.ahead, sync.behind), (0, 0));
}

#[test]
fn pull_merges_when_both_sides_have_new_commits_like_github_desktop() {
  let remote = Remote::new("diverged");
  commit_file(&remote.other, "b.txt", "b", "teammate");
  git(&remote.other, &["push"]);
  commit_file(&remote.mine, "c.txt", "c", "mine");
  vekro_git::fetch(&remote.mine, None, |_| {}).unwrap();
  let sync = vekro_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert_eq!((sync.ahead, sync.behind), (1, 1));

  vekro_git::pull(&remote.mine, None, |_| {}).unwrap();

  assert!(remote.mine.join("b.txt").exists() && remote.mine.join("c.txt").exists());
  let sync = vekro_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert_eq!((sync.ahead, sync.behind), (2, 0), "my commit and the merge commit are still to push");
  assert_eq!(git(&remote.mine, &["rev-list", "--parents", "-n", "1", "HEAD"]).split(' ').count(), 3, "HEAD is a merge commit");
}

#[test]
fn a_conflicting_pull_leaves_a_merge_to_resolve() {
  let remote = Remote::new("conflict");
  commit_file(&remote.other, "a.txt", "one\nTEAMMATE\nthree\n", "teammate edit");
  git(&remote.other, &["push"]);
  commit_file(&remote.mine, "a.txt", "one\nMINE\nthree\n", "my edit");

  let error = vekro_git::pull(&remote.mine, None, |_| {}).unwrap_err();

  assert!(error.to_lowercase().contains("conflict"), "{error}");
  let summary = vekro_git::summary(&remote.mine).unwrap();
  assert!(summary.merging);
  let state = vekro_git::read_merge(&remote.mine).unwrap();
  assert_eq!(state.files.len(), 1);
  assert_eq!(state.files[0].path, "a.txt");
}

#[test]
fn pull_with_local_changes_that_do_not_overlap_keeps_them() {
  let remote = Remote::new("dirty");
  commit_file(&remote.other, "b.txt", "b", "teammate");
  git(&remote.other, &["push"]);
  fs::write(remote.mine.join("a.txt"), "one\ntwo\nthree\nfour\n").unwrap();
  vekro_git::fetch(&remote.mine, None, |_| {}).unwrap();

  vekro_git::pull(&remote.mine, None, |_| {}).unwrap();

  assert!(remote.mine.join("b.txt").exists());
  assert_eq!(fs::read_to_string(remote.mine.join("a.txt")).unwrap(), "one\ntwo\nthree\nfour\n");
}

#[test]
fn a_new_branch_is_published_by_the_first_push() {
  let remote = Remote::new("publish");
  vekro_git::create_branch(&remote.mine, "feature").unwrap();
  commit_file(&remote.mine, "f.txt", "f", "feature work");
  let sync = vekro_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert!(!sync.upstream);

  vekro_git::push(&remote.mine, None, |_| {}).unwrap();

  let sync = vekro_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert!(sync.upstream);
  assert_eq!((sync.ahead, sync.behind), (0, 0));
  assert!(git(&remote.root.join("origin.git"), &["branch", "--list", "feature"]).contains("feature"));
}
