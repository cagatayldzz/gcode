use vekro_storage::{WindowMode, WindowPlacement};

fn temp_file(name: &str) -> std::path::PathBuf {
  std::env::temp_dir().join(format!("vekro-window-{name}-{}", std::process::id())).join("window.json")
}

#[test]
fn a_window_placement_survives_a_save_and_load() {
  let file = temp_file("roundtrip");
  let saved = WindowPlacement { x: 120.5, y: 80., width: 1500., height: 900., mode: WindowMode::Maximized };
  saved.save(&file).unwrap();
  assert_eq!(WindowPlacement::load(&file), Some(saved));
  std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
}

#[test]
fn a_missing_file_gives_no_placement() {
  assert_eq!(WindowPlacement::load(&temp_file("missing")), None);
}

#[test]
fn a_size_that_is_not_a_real_size_is_ignored() {
  let file = temp_file("tiny");
  std::fs::create_dir_all(file.parent().unwrap()).unwrap();
  std::fs::write(&file, r#"{"x":0,"y":0,"width":0,"height":0,"mode":"windowed"}"#).unwrap();
  assert_eq!(WindowPlacement::load(&file), None);
  std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
}
