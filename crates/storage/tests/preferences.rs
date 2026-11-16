use std::path::PathBuf;
use vekro_storage::{Preferences, ThemeChoice};

#[test]
fn preferences_survive_a_save_and_load() {
  let dir = std::env::temp_dir().join(format!("vekro-preferences-{}", std::process::id()));
  let file = dir.join("preferences.json");
  let saved = Preferences { theme: ThemeChoice::Dark, projects_dir: Some(PathBuf::from("/work")), code_font_size: 15 };
  saved.save(&file).unwrap();
  assert_eq!(Preferences::load(&file), saved);
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_or_damaged_preferences_give_the_defaults() {
  let dir = std::env::temp_dir().join(format!("vekro-preferences-bad-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  std::fs::write(dir.join("preferences.json"), "{ not json").unwrap();
  assert_eq!(Preferences::load(&dir.join("preferences.json")), Preferences::default());
  assert_eq!(Preferences::load(&dir.join("missing.json")), Preferences::default());
  std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_code_font_size_defaults_and_stays_within_its_range() {
  let dir = std::env::temp_dir().join(format!("vekro-preferences-size-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("preferences.json");
  assert_eq!(Preferences::default().code_font_size, 13);

  std::fs::write(&file, r#"{"theme": "dark"}"#).unwrap();
  assert_eq!(Preferences::load(&file).code_font_size, 13);
  std::fs::write(&file, r#"{"code_font_size": 400}"#).unwrap();
  assert_eq!(Preferences::load(&file).code_font_size, 24);
  std::fs::write(&file, r#"{"code_font_size": 2}"#).unwrap();
  assert_eq!(Preferences::load(&file).code_font_size, 9);
  std::fs::remove_dir_all(dir).unwrap();
}
