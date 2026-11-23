use ws_storage::{WindowMode, WindowPlacement};

fn temp_file(name: &str) -> std::path::PathBuf {
  std::env::temp_dir().join(format!("ws-window-{name}-{}", std::process::id())).join("window.json")
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

#[test]
fn malformed_and_incomplete_window_files_are_ignored() {
  let file = temp_file("invalid");
  std::fs::create_dir_all(file.parent().unwrap()).unwrap();
  for contents in ["{", "[]", r#"{"x":0,"y":0,"width":800}"#, r#"{"x":1e100,"y":0,"width":800,"height":600}"#] {
    std::fs::write(&file, contents).unwrap();
    assert_eq!(WindowPlacement::load(&file), None, "{contents}");
  }
  std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
}

#[test]
fn saving_again_replaces_the_previous_window_placement() {
  let file = temp_file("replace");
  let first = WindowPlacement { x: -400., y: 20., width: 800., height: 600., mode: WindowMode::Fullscreen };
  let second = WindowPlacement { x: 0., y: 0., width: 1000., height: 700., mode: WindowMode::Windowed };
  first.save(&file).unwrap();
  second.save(&file).unwrap();

  assert_eq!(WindowPlacement::load(&file), Some(second));
  std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
}
