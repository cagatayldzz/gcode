use std::{
  fs,
  os::unix::fs::PermissionsExt as _,
  path::{Path, PathBuf},
  process::Command,
};
use ws_git::{amend_commit, commit};

fn git(path: &Path, args: &[&str]) -> String {
  let output = Command::new("git").arg("-C").arg(path).args(args).output().unwrap();
  assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
  String::from_utf8(output.stdout).unwrap().trim_end().into()
}

fn repository() -> tempfile::TempDir {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  git(root, &["init", "--initial-branch=main"]);
  git(root, &["config", "user.name", "Test User"]);
  git(root, &["config", "user.email", "test@example.com"]);
  git(root, &["config", "commit.gpgsign", "false"]);
  git(root, &["config", "core.hooksPath", ".git/test-hooks"]);
  fs::create_dir_all(root.join(".git/test-hooks")).unwrap();
  fs::write(root.join("selected[1].txt"), "base").unwrap();
  fs::write(root.join("other.txt"), "base other").unwrap();
  git(root, &["add", "--", "."]);
  git(root, &["commit", "-m", "Initial commit"]);
  dir
}

fn hook(root: &Path, name: &str, script: &str) {
  let path = root.join(".git/test-hooks").join(name);
  fs::write(&path, format!("#!/bin/sh\nset -eu\n{script}\n")).unwrap();
  fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn rejecting_hooks_retain_the_original_head_and_index() {
  for (name, message) in [("pre-commit", "Pre-commit refused"), ("commit-msg", "Commit message refused")] {
    let dir = repository();
    let root = dir.path();
    hook(root, name, &format!("echo '{message}'\nexit 1"));
    fs::write(root.join("new.txt"), "new").unwrap();
    fs::write(root.join("other.txt"), "stage me").unwrap();
    git(root, &["add", "--", "other.txt"]);
    let head = git(root, &["rev-parse", "HEAD"]);
    let index = fs::read(root.join(".git/index")).unwrap();

    let result = commit(root, &[PathBuf::from("new.txt")], "Add a file", "").unwrap_err();

    assert!(result.contains(message), "{result}");
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert!(root.join("new.txt").exists());
    assert!(!root.join(".git/index.lock").exists());
  }
}

#[test]
fn hooks_can_rewrite_the_message_and_stage_fixes_without_overwriting_unrelated_stage() {
  let dir = repository();
  let root = dir.path();
  fs::write(root.join("selected[1].txt"), "working change").unwrap();
  fs::write(root.join("selected1.txt"), "not selected").unwrap();
  fs::write(root.join("other.txt"), "externally staged").unwrap();
  git(root, &["add", "--", "other.txt"]);
  fs::write(root.join("other.txt"), "unstaged too").unwrap();
  hook(
    root,
    "pre-commit",
    "printf fixed > 'selected[1].txt'\ngit --literal-pathspecs add -- 'selected[1].txt'\nprintf generated > generated.txt\ngit add -- generated.txt",
  );
  hook(root, "commit-msg", "printf 'Reviewed message' > \"$1\"");
  hook(root, "post-commit", "touch .git/post-commit-ran");

  let result = commit(root, &[PathBuf::from("selected[1].txt")], "Original message", "").unwrap();

  assert!(result.warning.is_none(), "{result:?}");
  assert_eq!(git(root, &["show", "HEAD:selected[1].txt"]), "fixed");
  assert_eq!(git(root, &["show", "HEAD:generated.txt"]), "generated");
  assert_eq!(git(root, &["show", "HEAD:other.txt"]), "base other");
  assert_eq!(git(root, &["show", ":other.txt"]), "externally staged");
  assert_eq!(git(root, &["diff", "--cached", "--name-only"]), "other.txt");
  assert_eq!(git(root, &["diff", "--name-only"]), "other.txt");
  assert_eq!(git(root, &["log", "-1", "--format=%B"]), "Reviewed message");
  assert!(root.join(".git/post-commit-ran").is_file());
}

#[test]
fn message_only_amend_runs_hooks_and_preserves_staged_changes() {
  let dir = repository();
  let root = dir.path();
  fs::write(root.join("other.txt"), "staged for later").unwrap();
  git(root, &["add", "--", "other.txt"]);
  let tree = git(root, &["rev-parse", "HEAD^{tree}"]);
  hook(root, "commit-msg", "printf 'Approved amendment' > \"$1\"");

  let result = amend_commit(root, &[], "New message", "").unwrap();

  assert!(result.warning.is_none(), "{result:?}");
  assert_eq!(git(root, &["rev-parse", "HEAD^{tree}"]), tree);
  assert_eq!(git(root, &["show", ":other.txt"]), "staged for later");
  assert_eq!(git(root, &["log", "-1", "--format=%B"]), "Approved amendment");
}

#[test]
fn a_signing_failure_cannot_create_an_unsigned_commit() {
  let dir = repository();
  let root = dir.path();
  git(root, &["config", "commit.gpgsign", "true"]);
  git(root, &["config", "gpg.format", "openpgp"]);
  git(root, &["config", "gpg.program", "/usr/bin/false"]);
  let head = git(root, &["rev-parse", "HEAD"]);
  let index = fs::read(root.join(".git/index")).unwrap();
  fs::write(root.join("selected[1].txt"), "change").unwrap();

  let error = commit(root, &[PathBuf::from("selected[1].txt")], "Must sign", "").unwrap_err();

  assert!(error.contains("sign"), "{error}");
  assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
  assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn a_locked_real_index_refuses_the_commit_before_running_hooks() {
  let dir = repository();
  let root = dir.path();
  let lock = root.join(".git/index.lock");
  fs::write(&lock, "another Git command").unwrap();
  fs::write(root.join("other.txt"), "new").unwrap();
  hook(root, "pre-commit", "touch .git/pre-commit-ran");

  assert!(commit(root, &[PathBuf::from("other.txt")], "Locked", "").unwrap_err().contains("index is locked"));
  assert_eq!(git(root, &["log", "-1", "--format=%s"]), "Initial commit");
  assert_eq!(fs::read_to_string(lock).unwrap(), "another Git command");
  assert!(!root.join(".git/pre-commit-ran").exists());
}

#[test]
fn an_index_failure_after_commit_reports_the_commit_and_preserves_the_original_index() {
  let dir = repository();
  let root = dir.path();
  fs::write(root.join("selected[1].txt"), "change").unwrap();
  let index = fs::read(root.join(".git/index")).unwrap();
  hook(root, "post-commit", "touch \"${GIT_INDEX_FILE}.lock\"");

  let result = commit(root, &[PathBuf::from("selected[1].txt")], "Saved", "").unwrap();

  assert!(git(root, &["rev-parse", "HEAD"]).starts_with(&result.id));
  assert!(result.warning.is_some_and(|warning| warning.contains("could not update the index")));
  assert_eq!(git(root, &["show", "HEAD:selected[1].txt"]), "change");
  assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
  assert!(!root.join(".git/index.lock").exists());
}

#[test]
fn configured_ssh_signing_produces_a_signed_commit() {
  let dir = repository();
  let root = dir.path();
  let key = root.join(".git/test-signing-key");
  let generated = Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-f"]).arg(&key).output().unwrap();
  assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
  git(root, &["config", "commit.gpgsign", "true"]);
  git(root, &["config", "gpg.format", "ssh"]);
  git(root, &["config", "gpg.ssh.program", "/usr/bin/ssh-keygen"]);
  git(root, &["config", "user.signingkey", key.to_str().unwrap()]);
  fs::write(root.join("selected[1].txt"), "sign this").unwrap();

  let result = commit(root, &[PathBuf::from("selected[1].txt")], "Signed", "").unwrap();

  assert!(result.warning.is_none(), "{result:?}");
  assert!(git(root, &["cat-file", "commit", "HEAD"]).contains("gpgsig -----BEGIN SSH SIGNATURE-----"));
}

#[test]
fn external_ref_change_during_a_hook_does_not_replace_the_other_git_commit() {
  let dir = repository();
  let root = dir.path();
  let first = git(root, &["rev-parse", "HEAD"]);
  git(root, &["commit", "--allow-empty", "-m", "Next commit"]);
  hook(root, "pre-commit", &format!("git update-ref HEAD {first}"));
  fs::write(root.join("selected[1].txt"), "change").unwrap();
  let index = fs::read(root.join(".git/index")).unwrap();

  let error = commit(root, &[PathBuf::from("selected[1].txt")], "My commit", "").unwrap_err();

  assert!(error.contains("HEAD moved"), "{error}");
  assert_eq!(git(root, &["rev-parse", "HEAD"]), first);
  assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn a_linked_worktree_preserves_its_staging_and_the_primary_index() {
  let dir = repository();
  let root = dir.path();
  let worktree = tempfile::tempdir().unwrap();
  let linked = worktree.path().join("worktree");
  git(root, &["worktree", "add", "-b", "linked", linked.to_str().unwrap()]);
  let primary_index = fs::read(root.join(".git/index")).unwrap();
  fs::write(linked.join("selected[1].txt"), "linked change").unwrap();
  fs::write(linked.join("other.txt"), "linked staged").unwrap();
  git(&linked, &["add", "--", "other.txt"]);

  let result = commit(&linked, &[PathBuf::from("selected[1].txt")], "Linked commit", "").unwrap();

  assert!(result.warning.is_none(), "{result:?}");
  assert_eq!(git(&linked, &["show", "HEAD:selected[1].txt"]), "linked change");
  assert_eq!(git(&linked, &["show", ":other.txt"]), "linked staged");
  assert_eq!(git(root, &["log", "-1", "--format=%s"]), "Initial commit");
  assert_eq!(fs::read(root.join(".git/index")).unwrap(), primary_index);
}

#[test]
fn the_published_index_remains_readable_when_git_uses_split_index() {
  let dir = repository();
  let root = dir.path();
  git(root, &["config", "core.splitIndex", "true"]);
  git(root, &["update-index", "--split-index"]);
  fs::write(root.join("selected[1].txt"), "split change").unwrap();
  fs::write(root.join("other.txt"), "split staged").unwrap();
  git(root, &["add", "--", "other.txt"]);

  let result = commit(root, &[PathBuf::from("selected[1].txt")], "Split index commit", "").unwrap();

  assert!(result.warning.is_none(), "{result:?}");
  assert_eq!(git(root, &["show", "HEAD:selected[1].txt"]), "split change");
  assert_eq!(git(root, &["show", ":other.txt"]), "split staged");
  assert_eq!(git(root, &["diff", "--cached", "--name-only"]), "other.txt");
}
