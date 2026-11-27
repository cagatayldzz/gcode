use std::{fs, path::Path, process::Command};

fn git(path: &Path, args: &[&str]) -> String {
  let output = Command::new("git").arg("-C").arg(path).args(args).output().unwrap();
  assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
  String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn agent_reviews_include_committed_and_working_changes_from_the_pinned_base() {
  let temp = tempfile::tempdir().unwrap();
  let source = temp.path().join("source");
  fs::create_dir(&source).unwrap();
  git(&source, &["init", "--initial-branch=main"]);
  git(&source, &["config", "user.email", "test@example.com"]);
  git(&source, &["config", "user.name", "Test"]);
  git(&source, &["config", "commit.gpgsign", "false"]);
  fs::write(source.join("shared.txt"), "original\n").unwrap();
  fs::write(source.join(".gitignore"), "ignored.txt\n").unwrap();
  git(&source, &["add", "."]);
  git(&source, &["commit", "-m", "base"]);
  let base = ws_git::head_commit_id(&source).unwrap();

  fs::write(source.join("shared.txt"), "new source tip\n").unwrap();
  git(&source, &["commit", "-am", "new tip"]);
  fs::write(source.join("source-only.txt"), "keep in source\n").unwrap();
  let first = ws_git::add_worktree_at_commit(&source, &temp.path().join("codex"), "agent/codex", &base).unwrap();
  let second = ws_git::add_worktree_at_commit(&source, &temp.path().join("claude"), "agent/claude", &base).unwrap();
  assert_eq!(git(&first, &["rev-parse", "HEAD"]), base);
  assert_eq!(git(&second, &["rev-parse", "HEAD"]), base);
  assert!(!first.join("source-only.txt").exists());
  assert_eq!(fs::read_to_string(first.join("shared.txt")).unwrap(), "original\n");

  fs::write(first.join("committed.txt"), "from the agent commit\n").unwrap();
  git(&first, &["add", "committed.txt"]);
  git(&first, &["commit", "-m", "agent commit"]);
  fs::write(first.join("untracked.txt"), "from the working tree\n").unwrap();
  fs::write(first.join("ignored.txt"), "ignore me\n").unwrap();
  fs::write(second.join("shared.txt"), "other agent\n").unwrap();

  let patch = ws_git::changes_since(&first, &base).unwrap();
  assert!(patch.contains("+from the agent commit"), "{patch}");
  assert!(patch.contains("+from the working tree"), "{patch}");
  assert!(!patch.contains("ignored.txt"), "{patch}");
  assert!(!patch.contains("other agent"), "{patch}");
  assert!(!patch.contains("new source tip"), "{patch}");
  assert_eq!(git(&source, &["status", "--porcelain"]), "?? source-only.txt");
}
