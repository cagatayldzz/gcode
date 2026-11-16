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
