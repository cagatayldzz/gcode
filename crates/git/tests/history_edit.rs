use std::{fs, path::Path, process::Command};

use ws_git::{Operation, OperationStep, ResetMode};

fn git(path: &Path, args: &[&str]) -> String {
  let output = Command::new("git").arg("-C").arg(path).args(args).output().unwrap();
  assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
  String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn init(path: &Path) -> String {
  git(path, &["init", "--initial-branch=main"]);
  git(path, &["config", "user.name", "Test"]);
  git(path, &["config", "user.email", "test@example.com"]);
  git(path, &["config", "commit.gpgsign", "false"]);
  commit(path, "file.txt", "original")
}

fn commit(path: &Path, file: &str, content: &str) -> String {
  fs::write(path.join(file), content).unwrap();
  git(path, &["add", "--", file]);
  git(path, &["commit", "-m", content]);
  git(path, &["rev-parse", "HEAD"])
}

#[test]
fn reset_modes_separate_branch_index_and_working_file_changes() {
  for (mode, index, working) in
    [(ResetMode::Soft, "staged", "working"), (ResetMode::Mixed, "original", "working"), (ResetMode::Hard, "original", "original")]
  {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    let first = init(path);
    let second = commit(path, "file.txt", "second");
    fs::write(path.join("file.txt"), "staged").unwrap();
    git(path, &["add", "file.txt"]);
    fs::write(path.join("file.txt"), "working").unwrap();
    fs::write(path.join("untracked.txt"), "keep").unwrap();

    ws_git::reset_to_commit(path, &ws_git::history_head(path).unwrap(), &first, mode).unwrap();
    assert_eq!(git(path, &["rev-parse", "HEAD"]), first);
    assert_eq!(git(path, &["rev-parse", "ORIG_HEAD"]), second);
    assert_eq!(git(path, &["show", ":file.txt"]), index);
    assert_eq!(fs::read_to_string(path.join("file.txt")).unwrap(), working);
    assert_eq!(fs::read_to_string(path.join("untracked.txt")).unwrap(), "keep");
  }
}

#[test]
fn confirmation_cannot_move_a_different_branch_at_the_same_commit() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path();
  let first = init(path);
  let second = commit(path, "file.txt", "second");
  let expected = ws_git::history_head(path).unwrap();
  git(path, &["switch", "-c", "other"]);

  assert!(ws_git::reset_to_commit(path, &expected, &first, ResetMode::Hard).unwrap_err().contains("HEAD changed"));
  assert_eq!(git(path, &["rev-parse", "HEAD"]), second);
  assert_eq!(fs::read_to_string(path.join("file.txt")).unwrap(), "second");
}

#[test]
fn rebase_replays_local_commits_onto_a_remote_tracking_ref() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path();
  init(path);
  git(path, &["branch", "topic"]);
  let upstream = commit(path, "base.txt", "base");
  git(path, &["update-ref", "refs/remotes/origin/main", &upstream]);
  git(path, &["switch", "topic"]);
  let original = commit(path, "topic.txt", "topic");
  // This branch would be rewritten by rebase.updateRefs without the explicit override.
  git(path, &["branch", "saved-topic"]);
  git(path, &["config", "rebase.updateRefs", "true"]);

  assert_eq!(ws_git::rebase_onto(path, &ws_git::history_head(path).unwrap(), "main", Some("origin")).unwrap(), Operation::Idle);
  assert_ne!(git(path, &["rev-parse", "HEAD"]), original);
  assert_eq!(git(path, &["rev-parse", "HEAD^"]), upstream);
  assert_eq!(git(path, &["rev-parse", "saved-topic"]), original);
  assert_eq!(fs::read_to_string(path.join("topic.txt")).unwrap(), "topic");
}

#[test]
fn conflicted_rebase_blocks_reset_and_can_be_aborted() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path();
  let first = init(path);
  git(path, &["branch", "topic"]);
  commit(path, "file.txt", "main edit");
  git(path, &["switch", "topic"]);
  let topic = commit(path, "file.txt", "topic edit");
  let expected = ws_git::history_head(path).unwrap();
  assert_eq!(ws_git::rebase_onto(path, &expected, "main", None).unwrap(), Operation::Rebase);
  assert!(ws_git::reset_to_commit(path, &expected, &first, ResetMode::Hard).unwrap_err().contains("Rebase in progress"));

  assert_eq!(ws_git::resume_operation(path, Operation::Rebase, OperationStep::Abort).unwrap(), Operation::Idle);
  assert_eq!(git(path, &["rev-parse", "HEAD"]), topic);
  assert_eq!(git(path, &["branch", "--show-current"]), "topic");
  assert_eq!(fs::read_to_string(path.join("file.txt")).unwrap(), "topic edit");
}

#[test]
fn worktrees_isolate_branches_and_keep_ignored_files_safe() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path().join("main tree");
  fs::create_dir(&root).unwrap();
  init(&root);
  commit(&root, ".gitignore", "local.txt\n");
  let linked = ws_git::add_worktree(&root, &dir.path().join("linked tree"), "feature/path").unwrap();
  let list = ws_git::worktrees(&root).unwrap();
  let tree = list.iter().find(|tree| tree.path == linked).unwrap();
  assert_eq!(tree.branch.as_deref(), Some("feature/path"));
  assert!(!tree.main);
  assert!(list.iter().any(|tree| tree.main && tree.branch.as_deref() == Some("main")));
  assert!(ws_git::switch_branch(&root, "feature/path").unwrap_err().contains("checked out"));
  assert!(ws_git::switch_branch(&linked, "main").unwrap_err().contains("checked out"));
  assert!(ws_git::delete_branch(&root, "feature/path").is_err());
  assert!(ws_git::remove_worktree(&linked, tree).is_err());
  git(&root, &["worktree", "lock", linked.to_str().unwrap()]);
  assert!(ws_git::remove_worktree(&root, tree).is_err());
  git(&root, &["worktree", "unlock", linked.to_str().unwrap()]);

  let ignored = linked.join("local.txt");
  fs::write(&ignored, "private notes").unwrap();
  assert!(ws_git::remove_worktree(&root, tree).unwrap_err().contains("local files"));
  assert_eq!(fs::read_to_string(&ignored).unwrap(), "private notes");
  fs::remove_file(ignored).unwrap();

  ws_git::remove_worktree(&root, tree).unwrap();
  assert!(!linked.exists());
  assert_eq!(ws_git::worktrees(&root).unwrap().len(), 1);
  let reopened = ws_git::add_worktree(&root, &dir.path().join("existing branch"), "feature/path").unwrap();
  assert_eq!(git(&reopened, &["branch", "--show-current"]), "feature/path");
  assert_eq!(git(&root, &["branch", "--show-current"]), "main");
}
