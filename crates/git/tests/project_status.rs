//! What a project's tab shows (changed files, commits waiting to be pushed) is read by path, for projects that are not the open one.
//! These tests follow a project through a push the way the interface does: read it, push it, read it again, with other projects around.

use std::{fs, path::Path, path::PathBuf, process::Command, sync::Arc, thread};

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

/// A bare "origin" and a clone of it, in one temporary folder that goes away with the value.
struct Project {
  root: PathBuf,
  work: PathBuf,
}

impl Project {
  fn new(name: &str) -> Self {
    let root = std::env::temp_dir().join(format!("ws-status-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "--bare", "--initial-branch=main", "origin.git"]);
    let origin = root.join("origin.git");
    git(&root, &["clone", origin.to_str().unwrap(), "seed"]);
    let seed = root.join("seed");
    git(&seed, &["checkout", "-b", "main"]);
    fs::write(seed.join("a.txt"), "one\n").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "first"]);
    git(&seed, &["push", "-u", "origin", "main"]);
    git(&root, &["clone", origin.to_str().unwrap(), "work"]);
    let work = root.join("work");
    git(&work, &["config", "user.name", "Test"]);
    git(&work, &["config", "user.email", "test@example.com"]);
    Self { root, work }
  }

  fn commit(&self, file: &str, text: &str, message: &str) {
    fs::write(self.work.join(file), text).unwrap();
    git(&self.work, &["add", "."]);
    git(&self.work, &["commit", "-m", message]);
  }

  fn status(&self) -> (usize, bool) {
    ws_git::quick_status(&self.work).expect("a repository")
  }

  fn push(&self) -> Result<String, String> {
    let result = ws_git::push(&self.work, None, |_| {})?;
    ws_git::fetch(&self.work, None, |_| {}).map_err(|error| format!("Push succeeded, but fetching failed: {error}"))?;
    Ok(result)
  }
}

impl Drop for Project {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.root);
  }
}

#[test]
fn a_clean_project_in_sync_with_its_remote_shows_nothing() {
  let project = Project::new("clean");
  assert_eq!(project.status(), (0, false));
}

#[test]
fn a_commit_shows_the_push_mark_and_a_push_with_the_fetch_after_it_clears_it() {
  let project = Project::new("push-clears");
  project.commit("b.txt", "two\n", "second");
  assert_eq!(project.status(), (0, true), "one commit is waiting");
  project.push().unwrap();
  assert_eq!(project.status(), (0, false), "the mark is gone as soon as push and fetch have ended");
}

#[test]
fn the_status_read_after_a_push_matches_what_the_open_project_would_show() {
  let project = Project::new("same-answer");
  project.commit("b.txt", "two\n", "second");
  project.push().unwrap();
  let from_summary = ws_git::summary(&project.work).unwrap().status();
  assert_eq!(ws_git::quick_status(&project.work), Some(from_summary));
}

#[test]
fn several_commits_are_one_mark_and_all_of_them_go_with_one_push() {
  let project = Project::new("several");
  for index in 0..3 {
    project.commit(&format!("f{index}.txt"), "x\n", &format!("commit {index}"));
  }
  assert_eq!(project.status(), (0, true));
  project.push().unwrap();
  assert_eq!(project.status(), (0, false));
}

#[test]
fn pushing_one_project_does_not_change_what_another_shows() {
  let first = Project::new("first");
  let second = Project::new("second");
  first.commit("b.txt", "two\n", "in the first");
  second.commit("c.txt", "three\n", "in the second");
  second.commit("d.txt", "four\n", "in the second again");
  first.push().unwrap();
  assert_eq!(first.status(), (0, false));
  assert_eq!(second.status(), (0, true), "the other project still waits to be pushed");
}

#[test]
fn uncommitted_changes_are_counted_and_survive_a_push() {
  let project = Project::new("dirty");
  project.commit("b.txt", "two\n", "second");
  fs::write(project.work.join("a.txt"), "changed\n").unwrap();
  fs::write(project.work.join("new.txt"), "new\n").unwrap();
  assert_eq!(project.status(), (2, true));
  project.push().unwrap();
  assert_eq!(project.status(), (2, false), "the push takes commits, not the working tree");
}

#[test]
fn a_branch_that_was_never_published_counts_as_waiting_until_its_first_push() {
  let project = Project::new("unpublished");
  git(&project.work, &["checkout", "-b", "feature"]);
  assert_eq!(project.status(), (0, true), "a new branch has nothing on the remote");
  project.push().unwrap();
  assert_eq!(project.status(), (0, false));
}

#[test]
fn a_refused_push_keeps_the_mark_so_the_tab_does_not_claim_the_work_was_sent() {
  let project = Project::new("refused");
  project.commit("b.txt", "two\n", "second");
  // The remote goes away: the push cannot succeed.
  fs::remove_dir_all(project.root.join("origin.git")).unwrap();
  assert!(project.push().is_err());
  assert_eq!(project.status(), (0, true));
}

#[test]
fn a_remote_that_moved_on_shows_no_push_mark_for_work_that_is_already_there() {
  let project = Project::new("moved-on");
  project.commit("b.txt", "two\n", "second");
  project.push().unwrap();
  // A teammate's commit arrives: the project is behind, not ahead.
  let other = project.root.join("other");
  git(&project.root, &["clone", project.root.join("origin.git").to_str().unwrap(), "other"]);
  fs::write(other.join("t.txt"), "t\n").unwrap();
  git(&other, &["add", "."]);
  git(&other, &["commit", "-m", "teammate"]);
  git(&other, &["push"]);
  ws_git::fetch(&project.work, None, |_| {}).unwrap();
  assert_eq!(project.status(), (0, false));
}

#[test]
fn a_status_read_while_a_push_runs_never_fails_and_ends_cleared() {
  let project = Arc::new(Project::new("concurrent"));
  for index in 0..5 {
    project.commit(&format!("f{index}.txt"), &"x".repeat(2000), &format!("commit {index}"));
  }
  let pushing = {
    let project = project.clone();
    thread::spawn(move || project.push())
  };
  // The tabs are read on another thread while the push runs: every read answers, whatever state the push is in.
  while !pushing.is_finished() {
    assert!(ws_git::quick_status(&project.work).is_some());
  }
  pushing.join().unwrap().unwrap();
  assert_eq!(project.status(), (0, false));
}

#[test]
fn a_folder_that_is_not_a_repository_has_no_status() {
  let dir = std::env::temp_dir().join(format!("ws-status-plain-{}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  assert_eq!(ws_git::quick_status(&dir), None);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_folder_inside_a_repository_reports_the_repository() {
  let project = Project::new("inside");
  project.commit("b.txt", "two\n", "second");
  let inside = project.work.join("sub");
  fs::create_dir_all(&inside).unwrap();
  assert_eq!(ws_git::quick_status(&inside), Some((0, true)));
}
