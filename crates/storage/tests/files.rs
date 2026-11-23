use ws_storage::{read_json, write_atomic, write_json, ProjectList};

fn temp_dir(name: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("ws-files-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

#[test]
fn an_atomic_write_replaces_the_file_and_leaves_no_temporary_file() {
  let dir = temp_dir("atomic");
  let file = dir.join("a.txt");
  write_atomic(&file, b"first").unwrap();
  write_atomic(&file, b"second").unwrap();
  assert_eq!(std::fs::read_to_string(&file).unwrap(), "second");
  assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_atomic_write_keeps_the_permissions_of_the_file() {
  use std::os::unix::fs::PermissionsExt;
  let dir = temp_dir("mode");
  let file = dir.join("script.sh");
  std::fs::write(&file, "old").unwrap();
  std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o750)).unwrap();
  write_atomic(&file, b"new").unwrap();
  assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o750);
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_atomic_write_keeps_a_symbolic_link_a_link() {
  let dir = temp_dir("link");
  let real = dir.join("real.txt");
  let link = dir.join("link.txt");
  std::fs::write(&real, "old").unwrap();
  std::os::unix::fs::symlink(&real, &link).unwrap();
  write_atomic(&link, b"new").unwrap();
  assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
  assert_eq!(std::fs::read_to_string(&real).unwrap(), "new");
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_json_write_keeps_the_previous_file_as_a_backup() {
  let dir = temp_dir("backup");
  let file = dir.join("data.json");
  write_json(&file, &serde_json::json!({ "n": 1 })).unwrap();
  write_json(&file, &serde_json::json!({ "n": 2 })).unwrap();
  assert_eq!(read_json(&file).unwrap()["n"], 2);
  assert_eq!(read_json(&dir.join("data.json.bak")).unwrap()["n"], 1);
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_corrupt_json_file_is_set_aside_and_the_backup_is_read() {
  let dir = temp_dir("corrupt");
  let file = dir.join("data.json");
  write_json(&file, &serde_json::json!({ "n": 1 })).unwrap();
  write_json(&file, &serde_json::json!({ "n": 2 })).unwrap();
  std::fs::write(&file, "{ broken").unwrap();
  assert_eq!(read_json(&file).unwrap()["n"], 1);
  assert_eq!(std::fs::read_to_string(dir.join("data.json.corrupt")).unwrap(), "{ broken");
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_corrupt_project_list_without_a_backup_loads_empty() {
  let dir = temp_dir("projects");
  let file = dir.join("projects.json");
  std::fs::write(&file, "not json").unwrap();
  assert!(ProjectList::load(&file).projects.is_empty());
  assert!(dir.join("projects.json.corrupt").exists());
  std::fs::remove_dir_all(dir).unwrap();
}
