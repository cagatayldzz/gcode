use std::path::PathBuf;
use ws_storage::ProjectList;

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
  let dir = std::env::temp_dir().join(format!("ws-projects-{}", std::process::id()));
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
fn dropping_a_project_on_another_starts_an_unnamed_category_and_survives_a_save() {
  let dir = std::env::temp_dir().join(format!("ws-categories-{}", std::process::id()));
  let file = dir.join("projects.json");
  let (a, b, c) = (PathBuf::from("/work/a"), PathBuf::from("/work/b"), PathBuf::from("/work/c"));
  let mut list = ProjectList::default();
  list.add_all([a.clone(), b.clone(), c.clone()]);

  let id = list.group(&b, &a).unwrap();
  assert_eq!(list.categories.len(), 1);
  assert_eq!(list.categories[0].name, "");
  assert_eq!(list.projects_in(Some(&id)).count(), 2);
  // A third project dropped on a member joins the same category.
  assert_eq!(list.group(&c, &a), Some(id.clone()));
  assert_eq!(list.projects_in(Some(&id)).count(), 3);
  assert_eq!(list.group(&a, &a), None);

  list.toggle_category(&id);
  list.save(&file).unwrap();
  assert_eq!(ProjectList::load(&file), list);
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_category_with_no_projects_left_is_deleted() {
  let (a, b) = (PathBuf::from("/work/a"), PathBuf::from("/work/b"));
  let mut list = ProjectList::default();
  list.add_all([a.clone(), b.clone()]);
  let id = list.group(&b, &a).unwrap();
  list.set_category(&a, None);
  assert_eq!(list.categories.len(), 1);
  list.set_category(&b, None);
  assert!(list.categories.is_empty());
  assert_eq!(list.projects_in(Some(&id)).count(), 0);
}

#[test]
fn renaming_and_deleting_a_category_keeps_the_projects() {
  let a = PathBuf::from("/work/a");
  let mut list = ProjectList::default();
  list.add(a.clone());
  let id = list.add_category("Work");
  list.set_category(&a, Some(&id));

  list.rename_category(&id, "Office");
  assert_eq!(list.categories[0].name, "Office");
  list.remove_category(&id);
  assert_eq!(list.projects_in(None).count(), 1);
  assert!(list.categories.is_empty());
}

#[test]
fn a_project_keeps_its_name_icon_and_color_across_a_save() {
  let dir = std::env::temp_dir().join(format!("ws-appearance-{}", std::process::id()));
  let file = dir.join("projects.json");
  let a = PathBuf::from("/work/atlas");
  let mut list = ProjectList::default();
  list.add(a.clone());
  assert_eq!(list.projects[0].name(), "atlas");

  list.set_appearance(&a, "  Maps  ", Some("rocket"), Some("#4d8dff"));
  assert_eq!(list.projects[0].name(), "Maps");
  list.save(&file).unwrap();
  assert_eq!(ProjectList::load(&file), list);

  // The folder's own name, or nothing, goes back to the default.
  list.set_appearance(&a, "atlas", None, None);
  assert_eq!(list.projects[0], ws_storage::Project::new(a));
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_list_of_plain_paths_from_an_older_version_still_loads() {
  let dir = std::env::temp_dir().join(format!("ws-legacy-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("projects.json");
  std::fs::write(&file, r#"{"projects": ["/work/a", "/work/b"], "active": "/work/b"}"#).unwrap();
  let list = ProjectList::load(&file);
  assert_eq!(list.projects.len(), 2);
  assert!(list.categories.is_empty());
  assert_eq!(list.active, Some(PathBuf::from("/work/b")));
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn projects_and_categories_move_within_their_own_group() {
  let (a, b, c, d) = (PathBuf::from("/work/a"), PathBuf::from("/work/b"), PathBuf::from("/work/c"), PathBuf::from("/work/d"));
  let mut list = ProjectList::default();
  list.add_all([a.clone(), b.clone(), c.clone(), d.clone()]);
  let first = list.group(&b, &d).unwrap();
  let second = list.group(&c, &a).unwrap();

  // b and d share a category, so b moves past d and not past a or c.
  list.move_project_by(&b, 1);
  let order: Vec<_> = list.projects.iter().map(|project| project.path.clone()).collect();
  assert!(order.iter().position(|path| *path == d) < order.iter().position(|path| *path == b));
  // Already last in its group: nothing happens.
  let before = list.clone();
  list.move_project_by(&b, 1);
  assert_eq!(list, before);

  list.move_category_by(&second, -1);
  assert_eq!(list.categories[0].id, second);
  assert_eq!(list.categories[1].id, first);
  list.move_category_by(&second, -1);
  assert_eq!(list.categories[0].id, second);
}

#[test]
fn only_projects_whose_folder_is_gone_are_reported_missing() {
  let dir = std::env::temp_dir().join(format!("ws-missing-{}", std::process::id()));
  let (kept, gone) = (dir.join("kept"), dir.join("gone"));
  std::fs::create_dir_all(&kept).unwrap();
  std::fs::create_dir_all(&gone).unwrap();
  let mut list = ProjectList::default();
  list.add_all([kept.clone(), gone.clone()]);
  assert!(list.missing_paths().is_empty());

  std::fs::remove_dir_all(&gone).unwrap();
  assert_eq!(list.missing_paths(), vec![gone]);
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_named_category_stays_when_it_is_empty_and_an_unnamed_one_does_not() {
  let (a, b) = (PathBuf::from("/work/a"), PathBuf::from("/work/b"));
  let mut list = ProjectList::default();
  list.add_all([a.clone(), b.clone()]);
  let named = list.add_category("Work");
  list.set_category(&a, Some(&named));
  list.set_category(&a, None);
  assert_eq!(list.categories.len(), 1);
  assert_eq!(list.projects_in(Some(&named)).count(), 0);

  let unnamed = list.group(&b, &a).unwrap();
  list.set_category(&a, None);
  list.set_category(&b, None);
  assert!(list.categories.iter().all(|category| category.id != unnamed));
}

#[test]
fn dropping_a_project_before_another_moves_it_into_that_category_and_place() {
  let (a, b, c) = (PathBuf::from("/work/a"), PathBuf::from("/work/b"), PathBuf::from("/work/c"));
  let mut list = ProjectList::default();
  list.add_all([a.clone(), b.clone(), c.clone()]);
  let id = list.add_category("Work");
  list.set_category(&b, Some(&id));

  assert!(list.move_project_before(&c, &b));
  let order: Vec<_> = list.projects.iter().map(|project| project.path.clone()).collect();
  assert_eq!(order, vec![a.clone(), c.clone(), b.clone()]);
  assert_eq!(list.projects.iter().find(|project| project.path == c).unwrap().category.as_deref(), Some(id.as_str()));
  assert!(!list.move_project_before(&c, &c));
  assert!(!list.move_project_before(&c, &PathBuf::from("/work/missing")));
}

#[test]
fn favorites_are_starred_listed_in_order_and_survive_a_save() {
  let mut list = ProjectList::default();
  for name in ["a", "b", "c"] {
    list.add(PathBuf::from(format!("/work/{name}")));
  }
  assert_eq!(list.favorites().count(), 0, "nothing is starred at first");
  list.set_favorite(&PathBuf::from("/work/c"), true);
  list.set_favorite(&PathBuf::from("/work/a"), true);
  list.set_favorite(&PathBuf::from("/work/missing"), true);
  assert_eq!(list.favorites().map(|project| project.path.clone()).collect::<Vec<_>>(), [PathBuf::from("/work/a"), PathBuf::from("/work/c")]);

  let dir = std::env::temp_dir().join(format!("ws-projects-favorites-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("projects.json");
  list.save(&file).unwrap();
  let loaded = ProjectList::load(&file);
  assert_eq!(loaded.favorites().count(), 2);
  list.set_favorite(&PathBuf::from("/work/a"), false);
  assert_eq!(list.favorites().count(), 1);
  std::fs::write(&file, r#"{"projects": ["/work/a", {"path": "/work/b"}]}"#).unwrap();
  assert_eq!(ProjectList::load(&file).favorites().count(), 0, "files from before favorites have none");
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn starred_branches_belong_to_their_project_and_survive_a_save() {
  let mut list = ProjectList::default();
  list.add(PathBuf::from("/work/a"));
  list.add(PathBuf::from("/work/b"));
  let (a, b) = (PathBuf::from("/work/a"), PathBuf::from("/work/b"));
  list.set_favorite_branch(&a, "develop", true);
  list.set_favorite_branch(&a, "develop", true);
  list.set_favorite_branch(&a, "feature/x", true);
  list.set_favorite_branch(&b, "main", true);
  list.set_favorite_branch(&PathBuf::from("/work/missing"), "main", true);
  assert_eq!(list.favorite_branches(&a), ["develop", "feature/x"], "a branch is starred once");
  assert_eq!(list.favorite_branches(&b), ["main"]);

  let dir = std::env::temp_dir().join(format!("ws-projects-branches-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("projects.json");
  list.save(&file).unwrap();
  let mut loaded = ProjectList::load(&file);
  assert_eq!(loaded.favorite_branches(&a), ["develop", "feature/x"]);
  loaded.set_favorite_branch(&a, "develop", false);
  assert_eq!(loaded.favorite_branches(&a), ["feature/x"]);
  std::fs::write(&file, r#"{"projects": ["/work/a", {"path": "/work/b"}]}"#).unwrap();
  assert!(ProjectList::load(&file).favorite_branches(&a).is_empty(), "files from before starred branches have none");
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn relocating_a_project_keeps_its_look_and_place() {
  let mut list = ProjectList::default();
  list.add(PathBuf::from("/work/a"));
  list.add(PathBuf::from("/work/b"));
  list.set_appearance(&PathBuf::from("/work/a"), "Alpha", Some("rocket"), Some("#ff0000"));
  list.set_favorite(&PathBuf::from("/work/a"), true);
  list.active = Some(PathBuf::from("/work/a"));
  assert!(list.relocate(&PathBuf::from("/work/a"), &PathBuf::from("/work/c")));
  assert_eq!(list.projects[0].path, PathBuf::from("/work/c"));
  assert_eq!(list.projects[0].name.as_deref(), Some("Alpha"));
  assert!(list.projects[0].favorite);
  assert_eq!(list.active, Some(PathBuf::from("/work/c")));
  assert!(!list.relocate(&PathBuf::from("/work/c"), &PathBuf::from("/work/b")));
  assert!(!list.relocate(&PathBuf::from("/work/missing"), &PathBuf::from("/work/d")));
}

#[test]
fn swapping_two_projects_exchanges_their_places_and_keeps_their_categories() {
  let mut list = ProjectList::default();
  for name in ["a", "b", "c"] {
    list.add(PathBuf::from(format!("/work/{name}")));
  }
  assert!(list.swap_projects(&PathBuf::from("/work/a"), &PathBuf::from("/work/c")));
  let order: Vec<_> = list.projects.iter().map(|project| project.path.clone()).collect();
  assert_eq!(order, [PathBuf::from("/work/c"), PathBuf::from("/work/b"), PathBuf::from("/work/a")]);
  assert!(!list.swap_projects(&PathBuf::from("/work/a"), &PathBuf::from("/work/a")));
  assert!(!list.swap_projects(&PathBuf::from("/work/a"), &PathBuf::from("/work/missing")));
}

#[test]
fn the_address_a_project_was_cloned_from_is_remembered_and_saved() {
  let dir = std::env::temp_dir().join(format!("ws-remote-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("projects.json");
  let mut list = ProjectList::default();
  list.add(PathBuf::from("/work/a"));
  assert!(list.set_remote_url(&PathBuf::from("/work/a"), "git@example.com:o/a.git"));
  // The same address again changes nothing; an empty one never erases what is known; an unlisted project is ignored.
  assert!(!list.set_remote_url(&PathBuf::from("/work/a"), "git@example.com:o/a.git"));
  assert!(!list.set_remote_url(&PathBuf::from("/work/a"), "  "));
  assert!(!list.set_remote_url(&PathBuf::from("/work/none"), "x"));
  list.save(&file).unwrap();
  let loaded = ProjectList::load(&file);
  assert_eq!(loaded.projects[0].remote_url.as_deref(), Some("git@example.com:o/a.git"));
  // It follows the project when the folder is relocated.
  let mut moved = loaded;
  assert!(moved.relocate(&PathBuf::from("/work/a"), &PathBuf::from("/work/b")));
  assert_eq!(moved.projects[0].remote_url.as_deref(), Some("git@example.com:o/a.git"));
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn editor_preferences_round_trip_and_older_projects_use_the_defaults() {
  use ws_storage::{LineEndings, ProjectEditorSettings};

  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("projects.json");
  std::fs::write(&path, r#"{"projects":["/old","/custom"]}"#).unwrap();
  let mut projects = ProjectList::load(&path);
  assert_eq!(projects.projects[0].editor, ProjectEditorSettings::default());
  let custom = ProjectEditorSettings { tab_size: 4, hard_tabs: true, line_endings: LineEndings::Crlf, format_on_save: true };
  projects.projects[1].editor = custom;
  projects.save(&path).unwrap();
  let reopened = ProjectList::load(&path);
  assert_eq!(reopened.projects[0].editor, ProjectEditorSettings::default());
  assert_eq!(reopened.projects[1].editor, custom);
  assert_eq!(LineEndings::Preserve.encode("a\r\nb\n"), "a\r\nb\n");
  assert_eq!(LineEndings::Lf.encode("a\r\nb\n"), "a\nb\n");
  assert_eq!(LineEndings::Crlf.encode("a\r\nb\n"), "a\r\nb\r\n");
}
