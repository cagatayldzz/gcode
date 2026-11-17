//! The Vekro application: startup, windows and key bindings. `apps/vekro` only calls [`run`].

use gpui_kit::{component::TitleBar, *};
use vekro_ui::{
  Assets, ClearBuffer, CloseTab, CopySelection, NewTerminal, NewWindow, OpenSettings, PasteClipboard, Quit, SaveFile, SplitDown, SplitRight,
  ToggleTerminal, Workspace, TOOLBAR_HEIGHT,
};

/// Starts the application and runs it until it quits.
pub fn run() {
  let app = gpui_kit::application().with_assets(Assets);
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
      KeyBinding::new("cmd-d", SplitRight, None),
      KeyBinding::new("cmd-shift-d", SplitDown, None),
      // The terminal's own: they act on the selection and the screen of the shell that has the keyboard.
      KeyBinding::new("cmd-c", CopySelection, Some("Terminal")),
      KeyBinding::new("cmd-v", PasteClipboard, Some("Terminal")),
      KeyBinding::new("cmd-k", ClearBuffer, Some("Terminal")),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &NewWindow, cx| open_window(cx));
    cx.on_window_closed(|cx, _| {
      if cx.windows().is_empty() {
        cx.quit();
      }
    })
    .detach();
    open_window(cx);
  });
}

fn open_window(cx: &mut App) {
  let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
  let options = WindowOptions {
    window_bounds: Some(WindowBounds::Windowed(bounds)),
    window_min_size: Some(size(px(800.), px(500.))),
    titlebar: Some(TitlebarOptions { traffic_light_position: Some(point(px(18.), TOOLBAR_HEIGHT / 2. - px(6.))), ..TitleBar::title_bar_options() }),
    ..TitleBar::window_options()
  };
  gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Workspace::new(window, cx))).expect("Failed to open the Vekro window");
}
