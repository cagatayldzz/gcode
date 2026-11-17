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
  vekro_git::push(&dir, None, |_| {}).unwrap();
  assert!(git2::Repository::open_bare(&remote).unwrap().find_reference("refs/heads/main").is_ok());
  fs::write(dir.join("b.txt"), "b").unwrap();
  commit_all(&dir, "second");
  vekro_git::push(&dir, None, |_| {}).unwrap();
  fs::remove_dir_all(dir).unwrap();
  fs::remove_dir_all(remote).unwrap();
}

#[test]
fn pushing_without_a_remote_reports_git_s_message() {
  let dir = repository("noremote");
  fs::write(dir.join("a.txt"), "a").unwrap();
  commit_all(&dir, "first");
  assert!(!vekro_git::push(&dir, None, |_| {}).unwrap_err().is_empty());
  fs::remove_dir_all(dir).unwrap();
}

fn remote_pair(name: &str) -> (PathBuf, PathBuf) {
  let remote = std::env::temp_dir().join(format!("vekro-git-sync-{name}-remote-{}", std::process::id()));
  let _ = fs::remove_dir_all(&remote);
  git2::Repository::init_opts(&remote, git2::RepositoryInitOptions::new().bare(true).initial_head("main")).unwrap();
  let dir = repository(&format!("sync-{name}"));
  fs::write(dir.join("a.txt"), "a").unwrap();
  commit_all(&dir, "first");
  git2::Repository::open(&dir).unwrap().remote("origin", remote.to_str().unwrap()).unwrap();
  (dir, remote)
}

#[test]
fn a_repository_without_a_remote_has_no_sync_status() {
  let dir = repository("nosync");
  fs::write(dir.join("a.txt"), "a").unwrap();
  commit_all(&dir, "first");
  assert_eq!(summary(&dir).unwrap().sync, None);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_branch_not_yet_pushed_has_no_upstream_and_then_counts_commits_ahead() {
  let (dir, remote) = remote_pair("ahead");
  let before = summary(&dir).unwrap().sync.unwrap();
  assert_eq!((before.remote.as_str(), before.upstream, before.ahead, before.behind), ("origin", false, 0, 0));
  vekro_git::push(&dir, None, |_| {}).unwrap();
  let pushed = summary(&dir).unwrap().sync.unwrap();
  assert!(pushed.upstream && pushed.ahead == 0);
  fs::write(dir.join("b.txt"), "b").unwrap();
  commit_all(&dir, "second");
  assert_eq!(summary(&dir).unwrap().sync.unwrap().ahead, 1);
  fs::remove_dir_all(dir).unwrap();
  fs::remove_dir_all(remote).unwrap();
}

#[test]
fn fetching_shows_new_remote_commits_as_behind_and_pulling_catches_up() {
  let (dir, remote) = remote_pair("behind");
  vekro_git::push(&dir, None, |_| {}).unwrap();
  // Another clone pushes a commit.
  let parent = std::env::temp_dir().join(format!("vekro-git-sync-parent2-{}", std::process::id()));
  let _ = fs::remove_dir_all(&parent);
  fs::create_dir_all(&parent).unwrap();
  let second = vekro_git::clone(remote.to_str().unwrap(), &parent, None).unwrap();
  let repository = git2::Repository::open(&second).unwrap();
  let mut config = repository.config().unwrap();
  config.set_str("user.name", "Test").unwrap();
  config.set_str("user.email", "test@example.com").unwrap();
  fs::write(second.join("c.txt"), "c").unwrap();
  commit_all(&second, "from elsewhere");
  vekro_git::push(&second, None, |_| {}).unwrap();

  assert_eq!(summary(&dir).unwrap().sync.unwrap().behind, 0);
  vekro_git::fetch(&dir, None, |_| {}).unwrap();
  let fetched = summary(&dir).unwrap().sync.unwrap();
  assert_eq!(fetched.behind, 1);
  assert!(fetched.last_fetched.is_some());
  vekro_git::pull(&dir, None, |_| {}).unwrap();
  assert_eq!(summary(&dir).unwrap().sync.unwrap().behind, 0);
  assert!(dir.join("c.txt").exists());
  for path in [dir, remote, parent] {
    fs::remove_dir_all(path).unwrap();
  }
}

#[test]
fn a_push_reports_git_s_progress_while_it_runs() {
  let (dir, remote) = remote_pair("progress");
  let mut lines = Vec::new();
  vekro_git::push(&dir, None, |line| lines.push(line.to_string())).unwrap();
  assert!(!lines.is_empty(), "no progress was reported");
  assert!(lines.iter().all(|line| !line.contains('\r') && !line.is_empty()));
  fs::remove_dir_all(dir).unwrap();
  fs::remove_dir_all(remote).unwrap();
}

#[test]
fn the_remote_url_and_the_last_commit_summary_are_read() {
  let (dir, remote) = remote_pair("url");
  assert_eq!(vekro_git::remote_url(&dir).as_deref(), remote.to_str());
  assert_eq!(vekro_git::head_summary(&dir).as_deref(), Some("first"));
  let plain = repository("nourl");
  assert_eq!(vekro_git::remote_url(&plain), None);
  fs::remove_dir_all(dir).unwrap();
  fs::remove_dir_all(remote).unwrap();
  fs::remove_dir_all(plain).unwrap();
}

#[test]
fn credentials_reach_git_through_the_helper_and_stay_out_of_debug_output() {
  let credentials = vekro_git::Credentials { username: "user".into(), password: "s3cret".into() };
  assert!(!format!("{credentials:?}").contains("s3cret"));
  // A local remote needs no login, so a push with credentials must simply work: the helper is only asked when a server wants it.
  let (dir, remote) = remote_pair("credentials");
  vekro_git::push(&dir, Some(&credentials), |_| {}).unwrap();
  assert!(git2::Repository::open_bare(&remote).unwrap().find_reference("refs/heads/main").is_ok());
  // Git itself asks the helper: `git credential fill` for a host answers with the user name and password.
  let mut child = std::process::Command::new("git")
    .args(["-c", "credential.helper=", "-c"])
    .arg(format!("credential.helper={}", vekro_git::CREDENTIAL_HELPER))
    .args(["credential", "fill"])
    .env("VEKRO_GIT_USERNAME", "user")
    .env("VEKRO_GIT_PASSWORD", "s3cret")
    .env("GIT_TERMINAL_PROMPT", "0")
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .spawn()
    .unwrap();
  std::io::Write::write_all(&mut child.stdin.take().unwrap(), b"protocol=https\nhost=example.com\n\n").unwrap();
  let output = child.wait_with_output().unwrap();
  let answer = String::from_utf8_lossy(&output.stdout);
  assert!(answer.contains("username=user") && answer.contains("password=s3cret"), "{answer}");
  fs::remove_dir_all(dir).unwrap();
  fs::remove_dir_all(remote).unwrap();
}
