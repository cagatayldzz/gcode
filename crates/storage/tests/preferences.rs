use std::path::PathBuf;
use ws_storage::{ActivityBarLayout, FontChoice, Preferences, ThemeChoice};

#[test]
fn preferences_survive_a_save_and_load() {
  let dir = std::env::temp_dir().join(format!("ws-preferences-{}", std::process::id()));
  let file = dir.join("preferences.json");
  let saved = Preferences {
    theme: ThemeChoice::Dark,
    font: FontChoice::Inter,
    projects_dir: Some(PathBuf::from("/work")),
    code_font_size: 15,
    explorer_font_size: 18,
    commit_instructions: Some("Be brief.".into()),
    check_updates: false,
    activity_bar: ActivityBarLayout { order: vec!["git".into(), "explorer".into()], hidden: vec!["agents".into()] },
  };
  saved.save(&file).unwrap();
  assert_eq!(Preferences::load(&file), saved);
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_or_damaged_preferences_give_the_defaults() {
  let dir = std::env::temp_dir().join(format!("ws-preferences-bad-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  std::fs::write(dir.join("preferences.json"), "{ not json").unwrap();
  assert_eq!(Preferences::load(&dir.join("preferences.json")), Preferences::default());
  assert_eq!(Preferences::load(&dir.join("missing.json")), Preferences::default());
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn font_sizes_default_and_stay_within_their_range() {
  let dir = std::env::temp_dir().join(format!("ws-preferences-size-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("preferences.json");
  assert_eq!(Preferences::default().code_font_size, 13);
  assert_eq!(Preferences::default().explorer_font_size, 14);

  std::fs::write(&file, r#"{"theme": "dark"}"#).unwrap();
  assert_eq!(Preferences::load(&file).code_font_size, 13);
  std::fs::write(&file, r#"{"code_font_size": 400}"#).unwrap();
  assert_eq!(Preferences::load(&file).code_font_size, 24);
  assert_eq!(Preferences::load(&file).explorer_font_size, 14);
  std::fs::write(&file, r#"{"code_font_size": 2, "explorer_font_size": 400}"#).unwrap();
  assert_eq!(Preferences::load(&file).code_font_size, 9);
  assert_eq!(Preferences::load(&file).explorer_font_size, 24);
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn commit_instructions_distinguish_the_built_in_text_from_none_at_all() {
  let dir = std::env::temp_dir().join(format!("ws-preferences-instructions-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("preferences.json");
  assert_eq!(Preferences::default().commit_instructions, None);

  for instructions in [None, Some(String::new()), Some("Short subjects.".to_string())] {
    let saved = Preferences { commit_instructions: instructions, ..Preferences::default() };
    saved.save(&file).unwrap();
    assert_eq!(Preferences::load(&file), saved);
  }
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn update_checks_are_on_unless_turned_off() {
  let dir = std::env::temp_dir().join(format!("ws-preferences-updates-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("preferences.json");
  assert!(Preferences::default().check_updates);
  std::fs::write(&file, r#"{"theme": "dark"}"#).unwrap();
  assert!(Preferences::load(&file).check_updates, "a file from before the setting keeps checking");
  std::fs::write(&file, r#"{"check_updates": false}"#).unwrap();
  assert!(!Preferences::load(&file).check_updates);
  std::fs::remove_dir_all(dir).unwrap();
}

const ITEMS: [&str; 4] = ["explorer", "search", "git", "terminal"];

#[test]
fn the_activity_bar_keeps_the_default_order_until_the_user_moves_something() {
  let layout = ActivityBarLayout::default();
  assert_eq!(layout.arranged(&ITEMS), ITEMS);
  assert_eq!(layout.visible(&ITEMS), ITEMS);
}

#[test]
fn moving_and_hiding_activity_buttons_changes_what_the_bar_shows() {
  let mut layout = ActivityBarLayout::default();
  layout.move_by(&ITEMS, "terminal", -3);
  assert_eq!(layout.arranged(&ITEMS), ["terminal", "explorer", "search", "git"]);
  layout.move_by(&ITEMS, "terminal", 1);
  layout.move_by(&ITEMS, "git", 10);
  assert_eq!(layout.arranged(&ITEMS), ["explorer", "terminal", "search", "git"]);
  layout.set_hidden("terminal", true);
  assert_eq!(layout.visible(&ITEMS), ["explorer", "search", "git"]);
  assert_eq!(layout.arranged(&ITEMS).len(), 4);
  layout.set_hidden("terminal", false);
  assert!(!layout.is_hidden("terminal"));
}

#[test]
fn unknown_activity_ids_are_ignored_and_new_buttons_come_last() {
  let layout = ActivityBarLayout { order: vec!["git".into(), "gone".into(), "explorer".into()], hidden: vec!["gone".into()] };
  assert_eq!(layout.arranged(&ITEMS), ["git", "explorer", "search", "terminal"]);
}

#[test]
fn the_font_defaults_to_geist_and_an_unknown_name_gives_it() {
  let dir = std::env::temp_dir().join(format!("ws-preferences-font-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("preferences.json");
  assert_eq!(Preferences::default().font, FontChoice::Geist);
  std::fs::write(&file, r#"{"font": "github"}"#).unwrap();
  assert_eq!(Preferences::load(&file).font, FontChoice::GitHub);
  std::fs::write(&file, r#"{"font": "comic"}"#).unwrap();
  assert_eq!(Preferences::load(&file).font, FontChoice::Geist);
  std::fs::remove_dir_all(dir).unwrap();
}
