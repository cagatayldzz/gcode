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
    let root = std::env::temp_dir().join(format!("ws-remote-{name}-{}", std::process::id()));
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
  ws_git::fetch(&remote.mine, None, |_| {}).unwrap();
  let sync = ws_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert_eq!((sync.ahead, sync.behind), (0, 1));
  ws_git::pull(&remote.mine, None, |_| {}).unwrap();
  assert!(remote.mine.join("b.txt").exists());
  let sync = ws_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert_eq!((sync.ahead, sync.behind), (0, 0));
}

#[test]
fn pull_merges_when_both_sides_have_new_commits_like_github_desktop() {
  let remote = Remote::new("diverged");
  commit_file(&remote.other, "b.txt", "b", "teammate");
  git(&remote.other, &["push"]);
  commit_file(&remote.mine, "c.txt", "c", "mine");
  ws_git::fetch(&remote.mine, None, |_| {}).unwrap();
  let sync = ws_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert_eq!((sync.ahead, sync.behind), (1, 1));

  ws_git::pull(&remote.mine, None, |_| {}).unwrap();

  assert!(remote.mine.join("b.txt").exists() && remote.mine.join("c.txt").exists());
  let sync = ws_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert_eq!((sync.ahead, sync.behind), (2, 0), "my commit and the merge commit are still to push");
  assert_eq!(git(&remote.mine, &["rev-list", "--parents", "-n", "1", "HEAD"]).split(' ').count(), 3, "HEAD is a merge commit");
}

#[test]
fn a_conflicting_pull_leaves_a_merge_to_resolve() {
  let remote = Remote::new("conflict");
  commit_file(&remote.other, "a.txt", "one\nTEAMMATE\nthree\n", "teammate edit");
  git(&remote.other, &["push"]);
  commit_file(&remote.mine, "a.txt", "one\nMINE\nthree\n", "my edit");

  let error = ws_git::pull(&remote.mine, None, |_| {}).unwrap_err();

  assert!(error.to_lowercase().contains("conflict"), "{error}");
  let summary = ws_git::summary(&remote.mine).unwrap();
  assert!(summary.merging);
  let state = ws_git::read_merge(&remote.mine).unwrap();
  assert_eq!(state.files.len(), 1);
  assert_eq!(state.files[0].path, "a.txt");
}

#[test]
fn pull_with_local_changes_that_do_not_overlap_keeps_them() {
  let remote = Remote::new("dirty");
  commit_file(&remote.other, "b.txt", "b", "teammate");
  git(&remote.other, &["push"]);
  fs::write(remote.mine.join("a.txt"), "one\ntwo\nthree\nfour\n").unwrap();
  ws_git::fetch(&remote.mine, None, |_| {}).unwrap();

  ws_git::pull(&remote.mine, None, |_| {}).unwrap();

  assert!(remote.mine.join("b.txt").exists());
  assert_eq!(fs::read_to_string(remote.mine.join("a.txt")).unwrap(), "one\ntwo\nthree\nfour\n");
}

#[test]
fn a_new_branch_is_published_by_the_first_push() {
  let remote = Remote::new("publish");
  ws_git::create_branch(&remote.mine, "feature").unwrap();
  commit_file(&remote.mine, "f.txt", "f", "feature work");
  let sync = ws_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert!(!sync.upstream);

  ws_git::push(&remote.mine, None, |_| {}).unwrap();

  let sync = ws_git::summary(&remote.mine).unwrap().sync.unwrap();
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
  ws_git::fetch(&remote.mine, None, |_| {}).unwrap();

  let entries = ws_git::branch_entries(&remote.mine).unwrap();
  let names: Vec<_> = entries.iter().map(|entry| (entry.name.as_str(), entry.remote.as_deref())).collect();
  assert_eq!(names, [("main", None), ("teammate-work", Some("origin"))]);

  ws_git::switch_branch(&remote.mine, "teammate-work").unwrap();

  assert_eq!(ws_git::summary(&remote.mine).unwrap().branch, "teammate-work");
  assert!(remote.mine.join("t.txt").exists());
  let sync = ws_git::summary(&remote.mine).unwrap().sync.unwrap();
  assert!(sync.upstream);
  assert_eq!((sync.ahead, sync.behind), (0, 0));
  assert!(ws_git::branch_entries(&remote.mine).unwrap().iter().all(|entry| entry.remote.is_none()));
}

#[test]
fn branches_say_when_they_last_changed_and_which_one_is_the_default() {
  let remote = Remote::new("branch-times");
  git(&remote.other, &["checkout", "-b", "teammate-work"]);
  commit_file(&remote.other, "t.txt", "t", "teammate branch");
  git(&remote.other, &["push", "-u", "origin", "teammate-work"]);
  ws_git::fetch(&remote.mine, None, |_| {}).unwrap();

  let entries = ws_git::branch_entries(&remote.mine).unwrap();
  let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
  assert!(
    entries.iter().all(|entry| entry.time.is_some_and(|time| (now - time).abs() < 600)),
    "the tip commit's time, for local and remote branches"
  );
  assert_eq!(entries.iter().filter(|entry| entry.default).map(|entry| entry.name.as_str()).collect::<Vec<_>>(), ["main"]);
}

#[test]
fn a_branch_is_renamed_and_deleted_but_not_the_one_checked_out() {
  let remote = Remote::new("rename-delete");
  ws_git::create_branch(&remote.mine, "old-name").unwrap();
  ws_git::switch_branch(&remote.mine, "main").unwrap();

  ws_git::rename_branch(&remote.mine, "old-name", "new-name").unwrap();
  assert!(ws_git::rename_branch(&remote.mine, "new-name", "main").unwrap_err().contains("already exists"));
  assert!(ws_git::rename_branch(&remote.mine, "new-name", "bad name").is_err());
  let names: Vec<_> = ws_git::branch_entries(&remote.mine).unwrap().into_iter().map(|entry| entry.name).collect();
  assert_eq!(names, ["main", "new-name"]);

  assert!(ws_git::delete_branch(&remote.mine, "main").unwrap_err().contains("cannot be deleted"));
  ws_git::delete_branch(&remote.mine, "new-name").unwrap();
  assert_eq!(ws_git::branch_entries(&remote.mine).unwrap().len(), 1);
}

#[test]
fn a_branch_is_deleted_on_the_remote_too() {
  let remote = Remote::new("delete-remote");
  ws_git::create_branch(&remote.mine, "gone").unwrap();
  commit_file(&remote.mine, "g.txt", "g", "work");
  ws_git::push(&remote.mine, None, |_| {}).unwrap();
  ws_git::switch_branch(&remote.mine, "main").unwrap();
  ws_git::delete_branch(&remote.mine, "gone").unwrap();

  ws_git::delete_remote_branch(&remote.mine, "origin", "gone", None, |_| {}).unwrap();

  assert!(git(&remote.root.join("origin.git"), &["branch", "--list", "gone"]).is_empty());
}

#[test]
fn leaving_changes_stashes_them_on_the_branch_and_they_can_be_restored() {
  let remote = Remote::new("leave");
  ws_git::create_branch(&remote.mine, "feature").unwrap();
  ws_git::switch_branch(&remote.mine, "main").unwrap();
  fs::write(remote.mine.join("a.txt"), "edited\n").unwrap();
  fs::write(remote.mine.join("new.txt"), "untracked").unwrap();

  ws_git::switch_branch_with(&remote.mine, "feature", ws_git::Changes::Leave).unwrap();

  let summary = ws_git::summary(&remote.mine).unwrap();
  assert_eq!(summary.branch, "feature");
  assert!(summary.changes.is_empty(), "the changes were set aside");
  assert!(ws_git::has_stash(&remote.mine, "main"));
  assert!(!ws_git::has_stash(&remote.mine, "feature"));

  ws_git::switch_branch_with(&remote.mine, "main", ws_git::Changes::Leave).unwrap();
  ws_git::restore_stash(&remote.mine, "main").unwrap();
  assert_eq!(fs::read_to_string(remote.mine.join("a.txt")).unwrap(), "edited\n");
  assert!(remote.mine.join("new.txt").exists());
  assert!(!ws_git::has_stash(&remote.mine, "main"));
}

#[test]
fn bringing_changes_applies_them_on_the_other_branch_even_when_they_would_collide() {
  let remote = Remote::new("bring");
  ws_git::create_branch(&remote.mine, "feature").unwrap();
  commit_file(&remote.mine, "a.txt", "one\ntwo\nthree\nfeature line\n", "feature edit");
  ws_git::switch_branch(&remote.mine, "main").unwrap();
  fs::write(remote.mine.join("b.txt"), "carried").unwrap();
  fs::write(remote.mine.join("a.txt"), "one\ntwo\nthree\nmain line\n").unwrap();
  assert!(ws_git::switch_branch(&remote.mine, "feature").unwrap_err().starts_with(ws_git::SWITCH_OVERWRITE));

  let error = ws_git::switch_branch_with(&remote.mine, "feature", ws_git::Changes::Bring).unwrap_err();

  assert!(error.contains("stay stashed on main"), "{error}");
  assert!(ws_git::has_stash(&remote.mine, "main"), "nothing was lost");

  ws_git::discard_stash(&remote.mine, "main").unwrap();
  assert!(!ws_git::has_stash(&remote.mine, "main"));
}

#[test]
fn bringing_changes_that_do_not_collide_carries_them_over() {
  let remote = Remote::new("bring-clean");
  ws_git::create_branch(&remote.mine, "feature").unwrap();
  ws_git::switch_branch(&remote.mine, "main").unwrap();
  fs::write(remote.mine.join("b.txt"), "carried").unwrap();

  ws_git::switch_branch_with(&remote.mine, "feature", ws_git::Changes::Bring).unwrap();

  assert_eq!(ws_git::summary(&remote.mine).unwrap().branch, "feature");
  assert_eq!(fs::read_to_string(remote.mine.join("b.txt")).unwrap(), "carried");
  assert!(!ws_git::has_stash(&remote.mine, "main"));
}

#[test]
fn merging_with_uncommitted_changes_that_do_not_overlap_works() {
  let remote = Remote::new("merge-dirty");
  ws_git::create_branch(&remote.mine, "feature").unwrap();
  commit_file(&remote.mine, "f.txt", "f", "feature work");
  ws_git::switch_branch(&remote.mine, "main").unwrap();
  fs::write(remote.mine.join("a.txt"), "one\ntwo\nthree\nlocal edit\n").unwrap();

  let outcome = ws_git::start_merge(&remote.mine, "feature", "main").unwrap();

  assert!(matches!(outcome, ws_git::MergeOutcome::Finished));
  assert!(remote.mine.join("f.txt").exists());
  assert_eq!(fs::read_to_string(remote.mine.join("a.txt")).unwrap(), "one\ntwo\nthree\nlocal edit\n");
}

#[test]
fn merging_over_uncommitted_changes_to_the_same_file_is_refused_and_loses_nothing() {
  let remote = Remote::new("merge-overlap");
  ws_git::create_branch(&remote.mine, "feature").unwrap();
  commit_file(&remote.mine, "a.txt", "one\ntwo\nthree\nfeature\n", "feature edit");
  ws_git::switch_branch(&remote.mine, "main").unwrap();
  fs::write(remote.mine.join("a.txt"), "one\ntwo\nthree\nmine\n").unwrap();

  let error = ws_git::start_merge(&remote.mine, "feature", "main").unwrap_err();

  assert!(error.contains("overwritten"), "{error}");
  assert_eq!(fs::read_to_string(remote.mine.join("a.txt")).unwrap(), "one\ntwo\nthree\nmine\n");
  assert!(!ws_git::summary(&remote.mine).unwrap().merging);
}
