use std::path::PathBuf;
use vekro_storage::ProjectList;

#[test]
fn adding_a_project_twice_keeps_one_entry_and_activates_it() {
  let mut list = ProjectList::default();
  list.add(PathBuf::from("/work/a"));
  list.add(PathBuf::from("/work/b"));
  list.add(PathBuf::from("/work/a"));
  assert_eq!(list.projects.len(), 2);
  assert_eq!(list.active, Some(PathBuf::from("/work/a")));
}

#[test]
fn removing_the_active_project_activates_the_first_remaining() {
  let mut list = ProjectList::default();
  list.add(PathBuf::from("/work/a"));
  list.add(PathBuf::from("/work/b"));
  list.remove(&PathBuf::from("/work/b"));
  assert_eq!(list.active, Some(PathBuf::from("/work/a")));
}

#[test]
fn the_list_survives_a_save_and_load() {
  let dir = std::env::temp_dir().join(format!("vekro-projects-{}", std::process::id()));
  let file = dir.join("projects.json");
  let mut list = ProjectList::default();
  list.add(PathBuf::from("/work/a"));
  list.save(&file).unwrap();
  assert_eq!(ProjectList::load(&file), list);
  assert_eq!(ProjectList::load(&dir.join("missing.json")), ProjectList::default());
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn adding_many_skips_listed_projects_and_keeps_the_open_one() {
  let mut list = ProjectList::default();
  list.add(PathBuf::from("/work/a"));
  let added = list.add_all([PathBuf::from("/work/a"), PathBuf::from("/work/b"), PathBuf::from("/work/c")]);
  assert_eq!(added, 2);
  assert_eq!(list.projects.len(), 3);
  assert_eq!(list.active, Some(PathBuf::from("/work/a")));
  let mut empty = ProjectList::default();
  empty.add_all([PathBuf::from("/work/b")]);
  assert_eq!(empty.active, Some(PathBuf::from("/work/b")));
}

#[test]
fn categories_group_projects_and_survive_a_save_and_load() {
  let dir = std::env::temp_dir().join(format!("vekro-categories-{}", std::process::id()));
  let file = dir.join("projects.json");
  let mut list = ProjectList::default();
  list.add(PathBuf::from("/work/a"));
  list.add(PathBuf::from("/work/b"));
  assert!(list.add_category("Work"));
  assert!(!list.add_category("Work"));
  assert!(!list.add_category("  "));
  list.set_category(&PathBuf::from("/work/a"), Some("Work"));
  list.set_category(&PathBuf::from("/work/b"), Some("Missing"));
  list.toggle_category("Work");

  assert_eq!(list.projects_in(Some("Work")).count(), 1);
  assert_eq!(list.projects_in(None).count(), 1);
  list.save(&file).unwrap();
  assert_eq!(ProjectList::load(&file), list);
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn renaming_and_deleting_a_category_keeps_the_projects() {
  let mut list = ProjectList::default();
  list.add(PathBuf::from("/work/a"));
  list.add_category("Work");
  list.add_category("Play");
  list.set_category(&PathBuf::from("/work/a"), Some("Work"));

  assert!(!list.rename_category("Work", "Play"));
  assert!(list.rename_category("Work", "Office"));
  assert_eq!(list.projects_in(Some("Office")).count(), 1);
  list.remove_category("Office");
  assert_eq!(list.projects_in(None).count(), 1);
  assert_eq!(list.categories.len(), 1);
}

#[test]
fn a_list_of_plain_paths_from_an_older_version_still_loads() {
  let dir = std::env::temp_dir().join(format!("vekro-legacy-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("projects.json");
  std::fs::write(&file, r#"{"projects": ["/work/a", "/work/b"], "active": "/work/b"}"#).unwrap();
  let list = ProjectList::load(&file);
  assert_eq!(list.projects.len(), 2);
  assert!(list.categories.is_empty());
  assert_eq!(list.active, Some(PathBuf::from("/work/b")));
  std::fs::remove_dir_all(dir).unwrap();
}
