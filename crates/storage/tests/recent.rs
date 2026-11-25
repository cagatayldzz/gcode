use std::path::{Path, PathBuf};
use ws_storage::RecentList;

#[test]
fn the_file_edited_last_comes_first_and_appears_once() {
  let mut list = RecentList::default();
  list.touch_file("default", Path::new("/work/a.rs"));
  list.touch_file("default", Path::new("/work/b.rs"));
  list.touch_file("default", Path::new("/work/a.rs"));
  let paths: Vec<_> = list.files.iter().map(|entry| entry.path.clone()).collect();
  assert_eq!(paths, [PathBuf::from("/work/a.rs"), PathBuf::from("/work/b.rs")]);
}

#[test]
fn the_same_path_in_two_profiles_is_two_entries() {
  let mut list = RecentList::default();
  list.touch_project("default", Path::new("/work/a"));
  list.touch_project("work", Path::new("/work/a"));
  assert_eq!(list.projects.len(), 2);
  assert_eq!(list.projects[0].profile, "work");
}

#[test]
fn only_the_newest_entries_are_kept() {
  let mut list = RecentList::default();
  for number in 0..50 {
    list.touch_file("default", Path::new(&format!("/work/{number}.rs")));
  }
  assert_eq!(list.files.len(), 20);
  assert_eq!(list.files[0].path, PathBuf::from("/work/49.rs"));
}

#[test]
fn forgetting_a_project_forgets_its_files_and_nothing_else() {
  let mut list = RecentList::default();
  list.touch_project("default", Path::new("/work/a"));
  list.touch_project("default", Path::new("/work/b"));
  list.touch_file("default", Path::new("/work/a/src/main.rs"));
  list.touch_file("default", Path::new("/work/b/src/main.rs"));
  list.forget_project("default", Path::new("/work/a"));
  assert_eq!(list.projects.len(), 1);
  assert_eq!(list.files.len(), 1);
  assert_eq!(list.files[0].path, PathBuf::from("/work/b/src/main.rs"));
}

#[test]
fn the_list_survives_a_save_and_load() {
  let dir = std::env::temp_dir().join(format!("ws-recent-{}", std::process::id()));
  let file = dir.join("recent.json");
  let mut list = RecentList::default();
  list.touch_project("default", Path::new("/work/a"));
  list.touch_file("work", Path::new("/work/a/x.rs"));
  list.save(&file).unwrap();
  assert_eq!(RecentList::load(&file), list);
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_menu_keeps_at_least_five_projects_and_gives_the_rest_to_files() {
  let mut list = RecentList::default();
  for number in 0..9 {
    list.touch_file("default", Path::new(&format!("/work/{number}.rs")));
    list.touch_project("default", Path::new(&format!("/work/p{number}")));
  }
  let (files, projects) = list.menu_entries(10, 5);
  assert_eq!((files.len(), projects.len()), (5, 5));
  assert_eq!(files[0].path, PathBuf::from("/work/8.rs"));

  let mut few = RecentList::default();
  few.touch_project("default", Path::new("/work/a"));
  for number in 0..20 {
    few.touch_file("default", Path::new(&format!("/work/{number}.rs")));
  }
  let (files, projects) = few.menu_entries(10, 5);
  assert_eq!((files.len(), projects.len()), (9, 1));
}

#[test]
fn a_menu_gives_the_places_projects_do_not_use_to_files_and_the_other_way_round() {
  let mut projects_only = RecentList::default();
  for number in 0..12 {
    projects_only.touch_project("default", Path::new(&format!("/work/p{number}")));
  }
  let (files, projects) = projects_only.menu_entries(10, 5);
  assert_eq!((files.len(), projects.len()), (0, 10));
}
