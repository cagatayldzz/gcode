//! The Vekro application: startup, windows and key bindings. `apps/vekro` only calls [`run`].

use gpui_kit::{component::TitleBar, *};
use vekro_storage::{WindowMode, WindowPlacement};
use vekro_ui::{
  Assets, ClearBuffer, CloseTab, CopySelection, NewTerminal, NewWindow, OpenSettings, PasteClipboard, Quit, SaveFile, SplitDown, SplitRight,
  ToggleTerminal, ViewOnHost, Workspace, TOOLBAR_HEIGHT,
};

/// Starts the application and runs it until it quits.
pub fn run() {
  let app = gpui_kit::application().with_assets(Assets);
  app.on_reopen(|cx| {
    if cx.windows().is_empty() {
      open_window(cx);
    }
    cx.activate(true);
  });
  app.run(move |cx| {
    gpui_kit::init(cx);
    vekro_ui::init(cx);
    cx.activate(true);
    cx.bind_keys([
      KeyBinding::new("cmd-q", Quit, None),
      KeyBinding::new("cmd-n", NewWindow, None),
      KeyBinding::new("cmd-,", OpenSettings, None),
      KeyBinding::new("cmd-t", NewTerminal, None),
      KeyBinding::new("cmd-s", SaveFile, None),
      KeyBinding::new("cmd-w", CloseTab, None),
      KeyBinding::new("cmd-j", ToggleTerminal, None),
      KeyBinding::new("cmd-shift-g", ViewOnHost, None),
      KeyBinding::new("cmd-d", SplitRight, None),
      KeyBinding::new("cmd-shift-d", SplitDown, None),
      // The terminal's own: they act on the selection and the screen of the shell that has the keyboard.
      KeyBinding::new("cmd-c", CopySelection, Some("Terminal")),
      KeyBinding::new("cmd-v", PasteClipboard, Some("Terminal")),
      KeyBinding::new("cmd-k", ClearBuffer, Some("Terminal")),
    ]);
    cx.set_menus(vekro_ui::menus(None));
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &NewWindow, cx| open_window(cx));
    open_window(cx);
  });
}

/// The window as it was when it was last closed, if it still fits on a screen that is connected; else a centered default.
fn initial_bounds(cx: &App) -> WindowBounds {
  let saved = WindowPlacement::load(&WindowPlacement::default_path()).and_then(|placement| {
    let bounds = Bounds::new(point(px(placement.x), px(placement.y)), size(px(placement.width), px(placement.height)));
    // Its center must be on a screen, so that a window saved on a display that is gone does not open out of sight.
    let visible = cx.displays().iter().any(|display| display.bounds().contains(&bounds.center()));
    visible.then_some(match placement.mode {
      WindowMode::Windowed => WindowBounds::Windowed(bounds),
      WindowMode::Maximized => WindowBounds::Maximized(bounds),
      WindowMode::Fullscreen => WindowBounds::Fullscreen(bounds),
    })
  });
  saved.unwrap_or_else(|| WindowBounds::Windowed(Bounds::centered(None, size(px(1280.), px(800.)), cx)))
}

fn open_window(cx: &mut App) {
  let options = WindowOptions {
    window_bounds: Some(initial_bounds(cx)),
    window_min_size: Some(size(px(800.), px(500.))),
    titlebar: Some(TitlebarOptions { traffic_light_position: Some(point(px(18.), TOOLBAR_HEIGHT / 2. - px(6.))), ..TitleBar::title_bar_options() }),
    ..TitleBar::window_options()
  };
  gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Workspace::new(window, cx))).expect("Failed to open the Vekro window");
}
