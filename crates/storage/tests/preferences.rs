use std::path::PathBuf;
use vekro_storage::{Preferences, ThemeChoice};

#[test]
fn preferences_survive_a_save_and_load() {
  let dir = std::env::temp_dir().join(format!("vekro-preferences-{}", std::process::id()));
  let file = dir.join("preferences.json");
  let saved = Preferences { theme: ThemeChoice::Dark, projects_dir: Some(PathBuf::from("/work")) };
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
