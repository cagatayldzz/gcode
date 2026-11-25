use std::{fs, path::Path, process::Command};
use ws_git::{commit_file_diff, file_diff};

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

fn numbered(count: usize, changed: Option<usize>) -> String {
  (1..=count).map(|line| if Some(line) == changed { "changed\n".to_string() } else { format!("line {line}\n") }).collect()
}

#[test]
fn a_one_file_diff_carries_the_unchanged_lines_far_from_the_change() {
  let folder = std::env::temp_dir().join(format!("ws-whole-file-{}", std::process::id()));
  let _ = fs::remove_dir_all(&folder);
  fs::create_dir_all(&folder).unwrap();
  let dir = folder.as_path();
  git(dir, &["init", "--initial-branch=main"]);
  fs::write(dir.join("long.txt"), numbered(200, None)).unwrap();
  git(dir, &["add", "."]);
  git(dir, &["commit", "-m", "first"]);
  fs::write(dir.join("long.txt"), numbered(200, Some(100))).unwrap();

  let working = file_diff(dir, Path::new("long.txt")).unwrap();
  assert!(working.contains(" line 1\n"), "the first line is context");
  assert!(working.contains(" line 200\n"), "the last line is context");
  assert!(working.contains("@@ -1,200 +1,200 @@"));

  git(dir, &["commit", "-am", "second"]);
  let id = git(dir, &["rev-parse", "HEAD"]);
  let commit = commit_file_diff(dir, &id, Path::new("long.txt")).unwrap();
  assert!(commit.contains(" line 1\n") && commit.contains(" line 200\n"));
  fs::remove_dir_all(dir).unwrap();
}
