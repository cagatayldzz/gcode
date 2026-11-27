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
    assert!(
      window.try_find(gpui_kit::SharedString::from(format!("close-tab:{}", path.display()))).is_some(),
      "after docking: {}",
      path.file_name().unwrap().to_string_lossy()
    );
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

#[gpui_kit::test]
fn project_editor_settings_save_changes_from_the_editor_page(cx: &mut TestAppContext) {
  let (data, handle) = open_window_with_project(cx);
  toggle_project_settings(cx, handle);
  cx.update_window(handle, |_, window, cx| {
    use gpui_kit::test::{TestEventExt as _, TestQueryExt as _};

    let page = gpui_kit::ElementId::from("project-settings-page-Editor");
    let point = window.elements().into_iter().find(|element| element.path().contains(&page)).expect("Editor page navigation label").bounds().center();
    window.pointer_move(point, None, cx);
    window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
    window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
    window.render_frame(cx);
    assert!(window.try_find("project-tab-width").is_some());
    window.click("project-format-on-save", cx);
  })
  .unwrap();
  cx.run_until_parked();

  let projects = ws_storage::ProjectList::load(&data.path().join("projects.json"));
  assert!(projects.projects[0].editor.format_on_save);
}

#[gpui_kit::test]
fn the_split_editor_saves_its_focused_file_with_the_terminal_docked(cx: &mut TestAppContext) {
  use gpui_kit::test::{TestEventExt as _, TestQueryExt as _};

  let data = scratch_data_dir();
  let project = data.path().join("project");
  fs::create_dir(&project).unwrap();
  let first = project.join("first.txt");
  let second = project.join("second.txt");
  fs::write(&first, "first source").unwrap();
  fs::write(&second, "second source").unwrap();
  let mut projects = ws_storage::ProjectList::default();
  projects.add(project);
  projects.save(&data.path().join("projects.json")).unwrap();
  allow_shell_threads(cx);
  cx.update(|cx| {
    gpui_kit::init(cx);
    ws_ui::init(cx);
    ws_ui::open_window(Profile::default_profile(), cx);
  });
  draw_every_window(cx);
  let handle = cx.windows()[0];
  let open = |cx: &mut TestAppContext, path: &Path| {
    cx.update_window(handle, |_, window, cx| {
      window.render_frame(cx);
      let row = gpui_kit::ElementId::from(gpui_kit::SharedString::from(format!("entry-{}", path.display())));
      let point = window.elements().into_iter().find(|element| element.path().contains(&row)).expect("file in the explorer").bounds().center();
      window.pointer_move(point, None, cx);
      window.pointer_down(point, gpui_kit::MouseButton::Left, cx);
      window.pointer_up(point, gpui_kit::MouseButton::Left, cx);
    })
    .unwrap();
    draw_every_window(cx);
  };

  open(cx, &first);
  cx.dispatch_action(handle, ws_ui::ToggleEditorSplit);
  draw_every_window(cx);
  open(cx, &second);
  cx.update_window(handle, |_, window, cx| window.input("edited ", cx)).unwrap();
  cx.dispatch_action(handle, ws_ui::SaveFile);
  draw_every_window(cx);
  assert!(fs::read_to_string(&second).unwrap().contains("edited "));
  assert_eq!(fs::read_to_string(&first).unwrap(), "first source");

  cx.update_window(handle, |_, window, cx| {
    window.render_frame(cx);
    for path in [&first, &second] {
      assert!(
        window.try_find(gpui_kit::SharedString::from(format!("close-tab:{}", path.display()))).is_some(),
        "before docking: {}",
        path.file_name().unwrap().to_string_lossy()
      );
    }
  })
  .unwrap();

  cx.dispatch_action(handle, ws_ui::ToggleTerminalDock);
  draw_every_window(cx);
  cx.update_window(handle, |_, window, cx| {
    window.render_frame(cx);
    assert!(window.try_find("terminal-dock-toggle").is_some());
    for path in [&first, &second] {
      assert!(
        window.try_find(gpui_kit::SharedString::from(format!("close-tab:{}", path.display()))).is_some(),
        "after docking: {}",
        path.file_name().unwrap().to_string_lossy()
      );
    }
  })
  .unwrap();

  cx.dispatch_action(handle, ws_ui::ToggleTerminalDock);
  draw_every_window(cx);
  cx.update_window(handle, |_, window, cx| {
    window.render_frame(cx);
    assert!(window.try_find("terminal-dock-toggle").is_some());
    assert!(window.try_find(gpui_kit::SharedString::from(format!("close-tab:{}", first.display()))).is_none());
  })
  .unwrap();
}

#[gpui_kit::test]
fn project_scripts_run_only_on_request_and_keep_the_terminal_output(cx: &mut TestAppContext) {
  let (data, handle) = open_window_with_project(cx);
  let project = data.path().join("project");
  let marker = project.join("script-ran");
  fs::write(project.join("Makefile"), "smoke:\n\tprintf ran > script-ran\n").unwrap();

  cx.dispatch_action(handle, ws_ui::ShowProjectScripts);
  draw_every_window(cx);
  assert!(!marker.exists());
  cx.update_window(handle, |_, window, cx| {
    window.render_frame(cx);
    window.click("script-Makefile-smoke-Run", cx);
  })
  .unwrap();
  allow_shell_threads(cx);
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
  while !marker.exists() && std::time::Instant::now() < deadline {
    std::thread::sleep(std::time::Duration::from_millis(10));
    cx.run_until_parked();
  }
  assert_eq!(fs::read_to_string(marker).unwrap(), "ran");
  draw_every_window(cx);

  cx.dispatch_action(handle, ws_ui::ShowProjectScripts);
  draw_every_window(cx);
  cx.update_window(handle, |_, window, cx| {
    window.render_frame(cx);
    assert!(window.try_find("script-Makefile-smoke-Show Output").is_some());
  })
  .unwrap();
}

#[gpui_kit::test]
fn the_worktree_manager_opens_a_linked_worktree_as_a_project(cx: &mut TestAppContext) {
  let (data, handle) = open_window_with_project(cx);
  let root = data.path().join("project");
  let git = |args: &[&str]| {
    let output = std::process::Command::new("git").arg("-C").arg(&root).args(args).output().unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
  };
  git(&["init", "--initial-branch=main"]);
  git(&["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false", "commit", "--allow-empty", "-m", "initial"]);
  let linked = ws_git::add_worktree(&root, &data.path().join("linked project"), "topic").unwrap();

  cx.dispatch_action(handle, ws_ui::ShowWorktrees);
  draw_every_window(cx);
  cx.update_window(handle, |_, window, cx| {
    window.render_frame(cx);
    assert!(window.try_find("add-worktree").is_some());
    window.click(format!("open-worktree-{}", linked.display()), cx);
  })
  .unwrap();
  draw_every_window(cx);

  let projects = ws_storage::ProjectList::load(&data.path().join("projects.json"));
  assert_eq!(projects.active.as_ref(), Some(&linked));
  assert!(projects.projects.iter().any(|project| project.path == root));
}
