//! Opens the real window headlessly and draws it, so a panic while building or drawing it (for example a style that is set
//! twice) fails here and not only when the app is launched.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext, TestAppContext};
use std::{
  fs,
  path::Path,
  sync::{Mutex, MutexGuard},
};
use ws_storage::Profile;

/// A fresh temporary data directory that the test owns until it is dropped. `<prefix>_DATA_DIR` is shared by the whole process, so
/// the tests take turns.
struct Scratch {
  dir: tempfile::TempDir,
  _turn: MutexGuard<'static, ()>,
}

impl Scratch {
  fn path(&self) -> &Path {
    self.dir.path()
  }
}

/// Points the data directory at a fresh temporary folder, so that the test never reads or writes the user's own data.
fn scratch_data_dir() -> Scratch {
  static TURN: Mutex<()> = Mutex::new(());
  let turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
  let dir = tempfile::tempdir().expect("create a data directory");
  std::env::set_var(ws_product::env_name("DATA_DIR"), dir.path());
  Scratch { dir, _turn: turn }
}

/// A window with a project starts a real shell, and its PTY thread wakes the foreground executor from outside the test thread.
/// The test scheduler reports that as non-determinism unless it is told that other threads are expected.
fn allow_shell_threads(cx: &TestAppContext) {
  cx.executor().allow_parking();
}

fn draw_every_window(cx: &mut TestAppContext) {
  cx.run_until_parked();
  for handle in cx.windows() {
    cx.update_window(handle, |_, window, cx| drop(window.draw(cx))).expect("draw the window");
  }
  cx.run_until_parked();
}

#[gpui_kit::test]
fn a_new_window_draws_without_a_project(cx: &mut TestAppContext) {
  let _data = scratch_data_dir();
  cx.update(|cx| {
    gpui_kit::init(cx);
    ws_ui::init(cx);
    ws_ui::open_window(Profile::default_profile(), cx);
  });
  draw_every_window(cx);
  assert_eq!(cx.windows().len(), 1);
}

#[gpui_kit::test]
fn a_window_with_a_project_draws_its_explorer(cx: &mut TestAppContext) {
  let data = scratch_data_dir();
  let project = data.path().join("project");
  fs::create_dir_all(project.join("src")).expect("create the project");
  fs::write(project.join("src/main.rs"), "fn main() {}\n").expect("write a file");
  fs::write(project.join("README.md"), "# Project\n").expect("write a file");
  fs::write(data.path().join("projects.json"), serde_json::json!({ "projects": [{ "path": project }], "active": project }).to_string())
    .expect("write the project list");
  allow_shell_threads(cx);
  cx.update(|cx| {
    gpui_kit::init(cx);
    ws_ui::init(cx);
    ws_ui::open_window(Profile::default_profile(), cx);
  });
  draw_every_window(cx);
  assert_eq!(cx.windows().len(), 1);
}

#[gpui_kit::test]
fn opening_a_profile_twice_keeps_one_window(cx: &mut TestAppContext) {
  let _data = scratch_data_dir();
  cx.update(|cx| {
    gpui_kit::init(cx);
    ws_ui::init(cx);
    ws_ui::open_window(Profile::default_profile(), cx);
  });
  draw_every_window(cx);
  cx.update(|cx| ws_ui::open_window(Profile::default_profile(), cx));
  draw_every_window(cx);
  assert_eq!(cx.windows().len(), 1);
}

#[gpui_kit::test]
fn a_window_moved_right_before_it_closes_is_still_remembered(cx: &mut TestAppContext) {
  let data = scratch_data_dir();
  cx.update(|cx| {
    gpui_kit::init(cx);
    ws_ui::init(cx);
    ws_ui::open_window(Profile::default_profile(), cx);
  });
  draw_every_window(cx);
  let handle = cx.windows()[0];
  cx.simulate_window_resize(handle, gpui_kit::size(gpui_kit::px(913.), gpui_kit::px(611.)));
  draw_every_window(cx);
  // Closed within the delay of the normal save: only the write on close can have stored it.
  cx.update_window(handle, |_, window, _| window.remove_window()).expect("close the window");
  cx.run_until_parked();
  cx.update(|_| {});
  let placement = ws_storage::WindowPlacement::load(&data.path().join("window.json")).expect("the placement was written on close");
  assert_eq!((placement.width, placement.height), (913., 611.));
}

/// Opens a window whose profile has one project, and returns it with the data directory that holds the project.
fn open_window_with_project(cx: &mut TestAppContext) -> (Scratch, gpui_kit::AnyWindowHandle) {
  let data = scratch_data_dir();
  let project = data.path().join("project");
  fs::create_dir_all(&project).expect("create the project");
  fs::write(data.path().join("projects.json"), serde_json::json!({ "projects": [{ "path": project }], "active": project }).to_string())
    .expect("write the project list");
  allow_shell_threads(cx);
  cx.update(|cx| {
    gpui_kit::init(cx);
    ws_ui::init(cx);
    ws_ui::open_window(Profile::default_profile(), cx);
  });
  draw_every_window(cx);
  let handle = cx.windows()[0];
  (data, handle)
}

/// Runs the action in the window the way a button or a shortcut does, then draws the result.
fn toggle_project_settings(cx: &mut TestAppContext, handle: gpui_kit::AnyWindowHandle) {
  cx.update_window(handle, |_, window, cx| window.dispatch_action(Box::new(ws_ui::ToggleProjectSettings), cx)).expect("dispatch");
  cx.run_until_parked();
  cx.update_window(handle, |_, window, cx| window.render_frame(cx)).expect("draw");
}

fn settings_view_is_open(cx: &mut TestAppContext, handle: gpui_kit::AnyWindowHandle) -> bool {
  cx.update_window(handle, |_, window, _| window.try_find("settings-save").is_some()).expect("look at the window")
}

#[gpui_kit::test]
fn the_project_settings_view_opens_and_closes_from_the_activity_bar(cx: &mut TestAppContext) {
  let (_data, handle) = open_window_with_project(cx);
  assert!(!settings_view_is_open(cx, handle), "the settings view is not open yet");
  toggle_project_settings(cx, handle);
  assert!(settings_view_is_open(cx, handle), "the settings view takes the main area");
  toggle_project_settings(cx, handle);
  assert!(!settings_view_is_open(cx, handle), "the button closes the settings view again");
}

#[gpui_kit::test]
fn go_to_file_opens_and_draws_its_list(cx: &mut TestAppContext) {
  let (data, handle) = open_window_with_project(cx);
  let project = data.path().join("project");
  fs::create_dir_all(project.join("src")).expect("create a folder");
  fs::write(project.join("src/main.rs"), "fn main() {}\n").expect("write a file");
  cx.update_window(handle, |_, window, cx| window.dispatch_action(Box::new(ws_ui::GoToFile), cx)).expect("dispatch");
  // The index is read in the background; the list is drawn once with nothing and once with the files.
  draw_every_window(cx);
  draw_every_window(cx);
  assert_eq!(cx.windows().len(), 1);
}

#[gpui_kit::test]
fn the_command_palette_opens_and_draws_its_list(cx: &mut TestAppContext) {
  let (_data, handle) = open_window_with_project(cx);
  cx.update_window(handle, |_, window, cx| window.dispatch_action(Box::new(ws_ui::OpenCommandPalette), cx)).expect("dispatch");
  draw_every_window(cx);
  draw_every_window(cx);
  assert_eq!(cx.windows().len(), 1);
}

#[gpui_kit::test]
fn recovered_edits_open_as_a_tab_and_require_approval_to_overwrite_a_changed_disk_file(cx: &mut TestAppContext) {
  let data = scratch_data_dir();
  let path = data.path().join("notes.txt");
  fs::write(&path, "before").unwrap();
  let base = ws_storage::FileVersion::read(&path).unwrap();
  ws_storage::RecoveryJournal::new(data.path()).store(&ws_storage::Draft::new(&path, "recovered edit".into(), base)).unwrap();
  fs::write(&path, "external edit").unwrap();
  cx.update(|cx| {
    gpui_kit::init(cx);
    ws_ui::init(cx);
    ws_ui::open_window(Profile::default_profile(), cx);
  });
  draw_every_window(cx);
  let handle = cx.windows()[0];
  cx.update_window(handle, |_, window, cx| {
    window.render_frame(cx);
    assert!(window.try_find(gpui_kit::SharedString::from(format!("close-tab:{}", path.display()))).is_some());
  })
  .unwrap();

  cx.dispatch_action(handle, ws_ui::SaveFile);
  draw_every_window(cx);
  cx.update_window(handle, |_, window, cx| {
    window.render_frame(cx);
    assert!(window.try_find("overwrite").is_some());
  })
  .unwrap();
  assert_eq!(fs::read_to_string(&path).unwrap(), "external edit");

  fs::write(&path, "second external edit").unwrap();
  cx.update_window(handle, |_, window, cx| window.click("overwrite", cx)).unwrap();
  draw_every_window(cx);
  cx.update_window(handle, |_, window, cx| {
    window.render_frame(cx);
    assert!(window.try_find("overwrite").is_some());
  })
  .unwrap();
  assert_eq!(fs::read_to_string(&path).unwrap(), "second external edit");

  cx.update_window(handle, |_, window, cx| window.click("overwrite", cx)).unwrap();
  draw_every_window(cx);
  assert_eq!(fs::read_to_string(&path).unwrap(), "recovered edit");
  cx.executor().advance_clock(std::time::Duration::from_millis(500));
  cx.run_until_parked();
  assert!(ws_storage::RecoveryJournal::new(data.path()).load().unwrap().0.is_empty());
}
