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

/// A server that answers every request with 403 and counts them.
fn refusing_server() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
  use std::io::{Read, Write};
  let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  let url = format!("http://{}/owner/repo.git", listener.local_addr().unwrap());
  let seen = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
  let counter = seen.clone();
  std::thread::spawn(move || {
    for stream in listener.incoming() {
      let Ok(mut stream) = stream else { break };
      let mut buffer = [0u8; 4096];
      let _ = stream.read(&mut buffer);
      counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
      let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    }
  });
  (url, seen)
}

#[test]
fn a_refused_token_is_followed_by_the_users_own_login_and_the_message_says_both_failed() {
  let (url, seen) = refusing_server();
  let dir = repository("refused");
  fs::write(dir.join("a.txt"), "a").unwrap();
  commit_all(&dir, "first");
  git2::Repository::open(&dir).unwrap().remote("origin", &url).unwrap();
  let credentials = vekro_git::Credentials { username: "me".into(), password: "tok".into() };

  let error = vekro_git::push(&dir, Some(&credentials), |_| {}).unwrap_err();
  assert!(error.contains("403") && error.contains("refused the token of the connected account") && error.contains("own Git login"), "{error}");
  assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 2, "the token was tried, then the user's own login");

  // Without a connected account there is only one attempt and the plain message.
  let error = vekro_git::push(&dir, None, |_| {}).unwrap_err();
  assert!(error.contains("403") && !error.contains("connected account"), "{error}");
  assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 3);
  fs::remove_dir_all(dir).unwrap();
}

/// A bare remote whose pre-receive hook says `output` and refuses the push, the way a host refuses a commit.
fn refusing_remote(name: &str, output: &str) -> PathBuf {
  let remote = std::env::temp_dir().join(format!("vekro-git-hook-{name}-{}", std::process::id()));
  let _ = fs::remove_dir_all(&remote);
  git2::Repository::init_opts(&remote, git2::RepositoryInitOptions::new().bare(true).initial_head("main")).unwrap();
  let hook = remote.join("hooks/pre-receive");
  fs::create_dir_all(remote.join("hooks")).unwrap();
  // The hook's first lines are progress reports, like a host's: they must not end up in the message.
  fs::write(
    &hook,
    format!("#!/bin/sh\necho 'Resolving deltas:  43% (20/46)' >&2\necho 'Resolving deltas:  45% (21/46)' >&2\necho '{output}' >&2\nexit 1\n"),
  )
  .unwrap();
  std::process::Command::new("chmod").arg("+x").arg(&hook).status().unwrap();
  remote
}

#[test]
fn a_rejected_push_gives_the_reason_not_git_s_last_line() {
  let remote = refusing_remote("reason", "protected branch: no pushes today");
  let dir = repository("reason");
  fs::write(dir.join("a.txt"), "a").unwrap();
  commit_all(&dir, "first");
  git2::Repository::open(&dir).unwrap().remote("origin", remote.to_str().unwrap()).unwrap();
  let error = vekro_git::push(&dir, None, |_| {}).unwrap_err();
  assert!(error.contains("[remote rejected]") && error.contains("protected branch: no pushes today"), "{error}");
  assert!(!error.contains("failed to push some refs") && !error.contains("Resolving deltas"), "{error}");
  assert!(!error.contains(vekro_git::WORKFLOWS_REFUSED));
  fs::remove_dir_all(dir).unwrap();
  fs::remove_dir_all(remote).unwrap();
}

#[test]
fn a_push_refused_for_workflow_files_says_which_permission_is_missing() {
  let remote = refusing_remote(
    "workflow",
    "refusing to allow a Personal Access Token to create or update workflow .github/workflows/ci.yml without workflow scope",
  );
  let dir = repository("workflow");
  fs::write(dir.join("a.txt"), "a").unwrap();
  commit_all(&dir, "first");
  git2::Repository::open(&dir).unwrap().remote("origin", remote.to_str().unwrap()).unwrap();
  let error = vekro_git::push(&dir, None, |_| {}).unwrap_err();
  assert!(error.contains(vekro_git::WORKFLOWS_REFUSED) && error.contains("`workflow` scope") && error.contains(".github/workflows"), "{error}");
  fs::remove_dir_all(dir).unwrap();
  fs::remove_dir_all(remote).unwrap();
}
