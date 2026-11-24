use std::{fs, os::unix::fs::PermissionsExt as _, path::Path, process::Command};
use ws_git::editor_diff::{apply_editor_hunk, editor_diff, HunkAction};

fn git(root: &Path, args: &[&str]) -> String {
  let result = Command::new("git")
    .current_dir(root)
    .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false"])
    .args(args)
    .output()
    .unwrap();
  assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
  String::from_utf8(result.stdout).unwrap()
}

#[test]
fn block_operations_leave_other_changes_intact_and_reject_a_stale_preview() {
  let root = std::env::temp_dir().join(format!("ws-editor-diff-{}", std::process::id()));
  fs::create_dir_all(&root).unwrap();
  git(&root, &["init"]);
  let path = root.join("code.txt");
  let original = (1..=30).map(|line| format!("line {line}\n")).collect::<String>();
  fs::write(&path, &original).unwrap();
  fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
  git(&root, &["add", "code.txt"]);
  git(&root, &["commit", "-m", "Initial"]);
  let modified = original.replace("line 2\n", "first change\n").replace("line 22\n", "second change\n");
  fs::write(&path, &modified).unwrap();

  let preview = editor_diff(&path, &modified).unwrap();
  assert_eq!(preview.changes.iter().map(|change| change.lines.clone()).collect::<Vec<_>>(), [1..2, 21..22]);
  assert!(preview.staged.is_empty());
  apply_editor_hunk(&path, &preview.unstaged, 2..3, false, HunkAction::Stage).unwrap();
  assert!(git(&root, &["ls-files", "--stage"]).starts_with("100755 "));
  let staged = git(&root, &["show", ":code.txt"]);
  assert!(staged.contains("first change"));
  assert!(!staged.contains("second change"));
  assert!(apply_editor_hunk(&path, &preview.unstaged, 22..23, false, HunkAction::Revert).is_err());

  let preview = editor_diff(&path, &modified).unwrap();
  apply_editor_hunk(&path, &preview.staged, 2..3, false, HunkAction::Unstage).unwrap();
  assert_eq!(git(&root, &["show", ":code.txt"]), original);
  let preview = editor_diff(&path, &modified).unwrap();
  apply_editor_hunk(&path, &preview.unstaged, 22..23, false, HunkAction::Revert).unwrap();
  assert_eq!(fs::read_to_string(&path).unwrap(), original.replace("line 2\n", "first change\n"));
  assert_ne!(fs::metadata(&path).unwrap().permissions().mode() & 0o111, 0);
  fs::remove_dir_all(root).unwrap();
}
