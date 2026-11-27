use std::{
  fs,
  path::{Path, PathBuf},
  process::Command,
};
use ws_git::{change_index, commit_staged, staging_diff, StageSide};

fn git(path: &Path, args: &[&str]) -> String {
  let output = Command::new("git").arg("-C").arg(path).args(args).output().unwrap();
  assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
  String::from_utf8(output.stdout).unwrap()
}

fn repository() -> (tempfile::TempDir, String) {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  git(root, &["init", "--initial-branch=main"]);
  git(root, &["config", "user.name", "Test"]);
  git(root, &["config", "user.email", "test@example.com"]);
  git(root, &["config", "commit.gpgsign", "false"]);
  git(root, &["config", "core.hooksPath", ".git/test-hooks"]);
  let text: String = (1..=32).map(|n| format!("line {n}\n")).collect();
  fs::write(root.join("file[1].txt"), &text).unwrap();
  fs::write(root.join("other.txt"), "initial").unwrap();
  git(root, &["add", "."]);
  git(root, &["commit", "-m", "Base"]);
  (dir, text)
}

#[test]
fn staging_one_hunk_can_be_committed_without_other_edits_to_the_same_file() {
  let (dir, base) = repository();
  let root = dir.path();
  let file = PathBuf::from("file[1].txt");
  let files = [file.clone()];
  let first_only = base.replace("line 2\n", "first edit\n");
  let both = first_only.replace("line 30\n", "second edit\n");
  fs::write(root.join(&file), &both).unwrap();
  let patch = staging_diff(root, &files, StageSide::Unstaged).unwrap();
  assert_eq!(patch.hunks().count(), 2);
  change_index(root, &files, StageSide::Unstaged, Some((&patch, 0))).unwrap();
  assert_eq!(git(root, &["show", ":file[1].txt"]), first_only);
  let summary = ws_git::summary(root).unwrap();
  assert!(summary.changes[0].staged && summary.changes[0].unstaged);

  let result = commit_staged(root, "Partial commit", "").unwrap();
  assert!(result.warning.is_none(), "{result:?}");
  assert_eq!(git(root, &["show", "HEAD:file[1].txt"]), first_only);
  assert_eq!(fs::read_to_string(root.join(&file)).unwrap(), both);
  assert_eq!(staging_diff(root, &files, StageSide::Staged).unwrap().hunks().count(), 0);
  assert_eq!(staging_diff(root, &files, StageSide::Unstaged).unwrap().hunks().count(), 1);
}

#[test]
fn unstaging_one_hunk_leaves_other_staged_hunks_and_the_worktree_untouched() {
  let (dir, base) = repository();
  let root = dir.path();
  let files = [PathBuf::from("file[1].txt")];
  let first_only = base.replace("line 2\n", "first edit\n");
  let both = first_only.replace("line 30\n", "second edit\n");
  fs::write(root.join(&files[0]), &both).unwrap();
  change_index(root, &files, StageSide::Unstaged, None).unwrap();
  fs::write(root.join("other.txt"), "staged elsewhere").unwrap();
  git(root, &["add", "--", "other.txt"]);

  let patch = staging_diff(root, &files, StageSide::Staged).unwrap();
  assert_eq!(patch.hunks().count(), 2);
  change_index(root, &files, StageSide::Staged, Some((&patch, 1))).unwrap();

  assert_eq!(git(root, &["show", ":file[1].txt"]), first_only);
  assert_eq!(git(root, &["show", ":other.txt"]), "staged elsewhere");
  assert_eq!(fs::read_to_string(root.join(&files[0])).unwrap(), both);
}

#[test]
fn an_edited_or_reindexed_diff_requires_a_fresh_selection() {
  let (dir, base) = repository();
  let root = dir.path();
  let files = [PathBuf::from("file[1].txt")];
  fs::write(root.join(&files[0]), base.replace("line 2\n", "old edit\n")).unwrap();
  let patch = staging_diff(root, &files, StageSide::Unstaged).unwrap();
  fs::write(root.join(&files[0]), base.replace("line 2\n", "new edit\n")).unwrap();

  assert!(change_index(root, &files, StageSide::Unstaged, Some((&patch, 0))).unwrap_err().contains("diff changed"));
  assert_eq!(git(root, &["show", ":file[1].txt"]), base);
}

#[test]
fn a_new_binary_file_can_be_staged_and_unstaged_before_the_first_commit() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  git(root, &["init", "--initial-branch=main"]);
  let files = [PathBuf::from("new.bin")];
  fs::write(root.join(&files[0]), [0, 255, 0]).unwrap();

  change_index(root, &files, StageSide::Unstaged, None).unwrap();
  assert_eq!(git(root, &["ls-files"]), "new.bin\n");
  change_index(root, &files, StageSide::Staged, None).unwrap();
  assert!(git(root, &["ls-files"]).is_empty());
  assert_eq!(fs::read(root.join(&files[0])).unwrap(), [0, 255, 0]);
}

#[test]
fn restaging_a_rename_updates_the_destination_when_the_source_is_already_out_of_the_index() {
  let (dir, _) = repository();
  let root = dir.path();
  let files = [PathBuf::from("renamed.txt"), PathBuf::from("file[1].txt")];
  git(root, &["mv", "file[1].txt", "renamed.txt"]);
  fs::write(root.join(&files[0]), "updated rename").unwrap();

  change_index(root, &files, StageSide::Unstaged, None).unwrap();
  assert_eq!(git(root, &["show", ":renamed.txt"]), "updated rename");

  change_index(root, &files, StageSide::Staged, None).unwrap();
  assert!(git(root, &["diff", "--cached", "--name-only"]).is_empty());
  assert_eq!(fs::read_to_string(root.join(&files[0])).unwrap(), "updated rename");
}

#[test]
fn changed_spans_tell_which_lines_each_hunk_changes_and_where_hunks_meet() {
  let (dir, base) = repository();
  let root = dir.path();
  let files = [PathBuf::from("file[1].txt")];
  let edited = base.replace("line 2\n", "first edit\n").replace("line 30\n", "");
  fs::write(root.join("file[1].txt"), &edited).unwrap();
  let spans = staging_diff(root, &files, StageSide::Unstaged).unwrap().changed_spans();
  assert_eq!(spans.len(), 2);
  assert_eq!((spans[0].old.clone(), spans[0].new.clone()), (2..3, 2..3));
  assert_eq!((spans[1].old.clone(), spans[1].new.clone()), (30..31, 30..30));
  assert!(spans[0].meets_old(&(2..3)) && !spans[0].meets_old(&(30..31)));
  assert!(spans[1].meets_new(&(30..30)) && spans[1].meets_new(&(30..31)));
}

#[test]
fn several_hunks_can_be_staged_in_one_step() {
  let (dir, base) = repository();
  let root = dir.path();
  let file = PathBuf::from("file[1].txt");
  let files = [file.clone()];
  let edited = base.replace("line 2\n", "first edit\n").replace("line 16\n", "middle edit\n").replace("line 30\n", "second edit\n");
  fs::write(root.join(&file), &edited).unwrap();
  let patch = staging_diff(root, &files, StageSide::Unstaged).unwrap();
  assert_eq!(patch.hunks().count(), 3);
  ws_git::change_index_hunks(root, &files, StageSide::Unstaged, Some((&patch, &[0, 2]))).unwrap();
  let expected = base.replace("line 2\n", "first edit\n").replace("line 30\n", "second edit\n");
  assert_eq!(git(root, &["show", ":file[1].txt"]), expected);
  assert_eq!(fs::read_to_string(root.join(&file)).unwrap(), edited);
}
