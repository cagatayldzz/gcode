use std::fs;
use ws_storage::migrate_legacy_data_dir;

// The test changes `HOME` for the whole process, so everything that depends on it is one test.
#[test]
fn data_saved_under_the_previous_name_moves_once_and_never_over_newer_data() {
  let home = std::env::temp_dir().join(format!("ws-legacy-data-{}", std::process::id()));
  let _ = fs::remove_dir_all(&home);
  let support = home.join("Library").join("Application Support");
  fs::create_dir_all(&support).unwrap();
  std::env::set_var("HOME", &home);
  std::env::remove_var(ws_product::env_name("DATA_DIR"));

  assert!(!migrate_legacy_data_dir().unwrap(), "nothing to move without old data");

  let legacy = support.join(ws_product::LEGACY_NAME);
  fs::create_dir_all(&legacy).unwrap();
  fs::write(legacy.join("projects.json"), "{\"projects\":[]}").unwrap();
  assert!(migrate_legacy_data_dir().unwrap(), "the old folder moves");
  assert!(!legacy.exists());
  assert_eq!(fs::read_to_string(support.join(ws_product::NAME).join("projects.json")).unwrap(), "{\"projects\":[]}");
  assert!(!migrate_legacy_data_dir().unwrap(), "it moves once");

  // Data under the current name wins: an old folder that shows up later is left alone.
  fs::create_dir_all(&legacy).unwrap();
  fs::write(legacy.join("projects.json"), "old").unwrap();
  assert!(!migrate_legacy_data_dir().unwrap());
  assert_eq!(fs::read_to_string(legacy.join("projects.json")).unwrap(), "old");
  assert_eq!(fs::read_to_string(support.join(ws_product::NAME).join("projects.json")).unwrap(), "{\"projects\":[]}");

  // A data folder named by the environment is never replaced.
  std::env::set_var(ws_product::env_name("DATA_DIR"), home.join("elsewhere"));
  assert!(!migrate_legacy_data_dir().unwrap());
  std::env::remove_var(ws_product::env_name("DATA_DIR"));
  fs::remove_dir_all(home).unwrap();
}
