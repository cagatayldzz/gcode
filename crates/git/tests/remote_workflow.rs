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
    // The library reads the identity from the repository: CI machines have none of their own.
    for clone in ["mine", "other"] {
      git(&root.join(clone), &["config", "user.name", "Test"]);
      git(&root.join(clone), &["config", "user.email", "test@example.com"]);
    }
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

#[test]
fn remote_only_branches_are_listed_and_switching_creates_a_tracking_branch() {
  let remote = Remote::new("remote-branches");
  git(&remote.other, &["checkout", "-b", "teammate-work"]);
  commit_file(&remote.other, "t.txt", "t", "teammate branch");
  git(&remote.other, &["push", "-u", "origin", "teammate-work"]);
  vekro_git::fetch(&remote.mine, None, |_| {}).unwrap();

  let entries = vekro_git::branch_entries(&remote.mine).unwrap();
  let names: Vec<_> = entries.iter().map(|entry| (entry.name.as_str(), entry.remote.as_deref())).collect();
  assert_eq!(names, [("main", None), ("teammate-work", Some("origin"))]);

  vekro_git::switch_branch(&remote.mine, "teammate-work").unwrap();

  assert_eq!(vekro_git::summary(&remote.mine).unwrap().branch, "teammate-work");
  assert!(remote.mine.join("t.txt").exists());
  let sync = vekro_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert!(sync.upstream);
  assert_eq!((sync.ahead, sync.behind), (0, 0));
  assert!(vekro_git::branch_entries(&remote.mine).unwrap().iter().all(|entry| entry.remote.is_none()));
}

#[test]
fn a_branch_is_renamed_and_deleted_but_not_the_one_checked_out() {
  let remote = Remote::new("rename-delete");
  vekro_git::create_branch(&remote.mine, "old-name").unwrap();
  vekro_git::switch_branch(&remote.mine, "main").unwrap();

  vekro_git::rename_branch(&remote.mine, "old-name", "new-name").unwrap();
  assert!(vekro_git::rename_branch(&remote.mine, "new-name", "main").unwrap_err().contains("already exists"));
  assert!(vekro_git::rename_branch(&remote.mine, "new-name", "bad name").is_err());
  let names: Vec<_> = vekro_git::branch_entries(&remote.mine).unwrap().into_iter().map(|entry| entry.name).collect();
  assert_eq!(names, ["main", "new-name"]);

  assert!(vekro_git::delete_branch(&remote.mine, "main").unwrap_err().contains("cannot be deleted"));
  vekro_git::delete_branch(&remote.mine, "new-name").unwrap();
  assert_eq!(vekro_git::branch_entries(&remote.mine).unwrap().len(), 1);
}

#[test]
fn a_branch_is_deleted_on_the_remote_too() {
  let remote = Remote::new("delete-remote");
  vekro_git::create_branch(&remote.mine, "gone").unwrap();
  commit_file(&remote.mine, "g.txt", "g", "work");
  vekro_git::push(&remote.mine, None, |_| {}).unwrap();
  vekro_git::switch_branch(&remote.mine, "main").unwrap();
  vekro_git::delete_branch(&remote.mine, "gone").unwrap();

  vekro_git::delete_remote_branch(&remote.mine, "origin", "gone", None, |_| {}).unwrap();

  assert!(git(&remote.root.join("origin.git"), &["branch", "--list", "gone"]).is_empty());
}

#[test]
fn leaving_changes_stashes_them_on_the_branch_and_they_can_be_restored() {
  let remote = Remote::new("leave");
  vekro_git::create_branch(&remote.mine, "feature").unwrap();
  vekro_git::switch_branch(&remote.mine, "main").unwrap();
  fs::write(remote.mine.join("a.txt"), "edited\n").unwrap();
  fs::write(remote.mine.join("new.txt"), "untracked").unwrap();

  vekro_git::switch_branch_with(&remote.mine, "feature", vekro_git::Changes::Leave).unwrap();

  let summary = vekro_git::summary(&remote.mine).unwrap();
  assert_eq!(summary.branch, "feature");
  assert!(summary.changes.is_empty(), "the changes were set aside");
  assert!(vekro_git::has_stash(&remote.mine, "main"));
  assert!(!vekro_git::has_stash(&remote.mine, "feature"));

  vekro_git::switch_branch_with(&remote.mine, "main", vekro_git::Changes::Leave).unwrap();
  vekro_git::restore_stash(&remote.mine, "main").unwrap();
  assert_eq!(fs::read_to_string(remote.mine.join("a.txt")).unwrap(), "edited\n");
  assert!(remote.mine.join("new.txt").exists());
  assert!(!vekro_git::has_stash(&remote.mine, "main"));
}

#[test]
fn bringing_changes_applies_them_on_the_other_branch_even_when_they_would_collide() {
  let remote = Remote::new("bring");
  vekro_git::create_branch(&remote.mine, "feature").unwrap();
  commit_file(&remote.mine, "a.txt", "one\ntwo\nthree\nfeature line\n", "feature edit");
  vekro_git::switch_branch(&remote.mine, "main").unwrap();
  fs::write(remote.mine.join("b.txt"), "carried").unwrap();
  fs::write(remote.mine.join("a.txt"), "one\ntwo\nthree\nmain line\n").unwrap();
  assert!(vekro_git::switch_branch(&remote.mine, "feature").unwrap_err().starts_with(vekro_git::SWITCH_OVERWRITE));

  let error = vekro_git::switch_branch_with(&remote.mine, "feature", vekro_git::Changes::Bring).unwrap_err();

  assert!(error.contains("stay stashed on main"), "{error}");
  assert!(vekro_git::has_stash(&remote.mine, "main"), "nothing was lost");

  vekro_git::discard_stash(&remote.mine, "main").unwrap();
  assert!(!vekro_git::has_stash(&remote.mine, "main"));
}

#[test]
fn bringing_changes_that_do_not_collide_carries_them_over() {
  let remote = Remote::new("bring-clean");
  vekro_git::create_branch(&remote.mine, "feature").unwrap();
  vekro_git::switch_branch(&remote.mine, "main").unwrap();
  fs::write(remote.mine.join("b.txt"), "carried").unwrap();

  vekro_git::switch_branch_with(&remote.mine, "feature", vekro_git::Changes::Bring).unwrap();

  assert_eq!(vekro_git::summary(&remote.mine).unwrap().branch, "feature");
  assert_eq!(fs::read_to_string(remote.mine.join("b.txt")).unwrap(), "carried");
  assert!(!vekro_git::has_stash(&remote.mine, "main"));
}

#[test]
fn merging_with_uncommitted_changes_that_do_not_overlap_works() {
  let remote = Remote::new("merge-dirty");
  vekro_git::create_branch(&remote.mine, "feature").unwrap();
  commit_file(&remote.mine, "f.txt", "f", "feature work");
  vekro_git::switch_branch(&remote.mine, "main").unwrap();
  fs::write(remote.mine.join("a.txt"), "one\ntwo\nthree\nlocal edit\n").unwrap();

  let outcome = vekro_git::start_merge(&remote.mine, "feature", "main").unwrap();

  assert!(matches!(outcome, vekro_git::MergeOutcome::Finished));
  assert!(remote.mine.join("f.txt").exists());
  assert_eq!(fs::read_to_string(remote.mine.join("a.txt")).unwrap(), "one\ntwo\nthree\nlocal edit\n");
}

#[test]
fn merging_over_uncommitted_changes_to_the_same_file_is_refused_and_loses_nothing() {
  let remote = Remote::new("merge-overlap");
  vekro_git::create_branch(&remote.mine, "feature").unwrap();
  commit_file(&remote.mine, "a.txt", "one\ntwo\nthree\nfeature\n", "feature edit");
  vekro_git::switch_branch(&remote.mine, "main").unwrap();
  fs::write(remote.mine.join("a.txt"), "one\ntwo\nthree\nmine\n").unwrap();

  let error = vekro_git::start_merge(&remote.mine, "feature", "main").unwrap_err();

  assert!(error.contains("overwritten"), "{error}");
  assert_eq!(fs::read_to_string(remote.mine.join("a.txt")).unwrap(), "one\ntwo\nthree\nmine\n");
  assert!(!vekro_git::summary(&remote.mine).unwrap().merging);
}
