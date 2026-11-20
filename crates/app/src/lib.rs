//! The Vekro application: startup, windows and key bindings. `apps/vekro` only calls [`run`].

use gpui_kit::*;
use vekro_ui::{
  Assets, ClearBuffer, CloseTab, CopySelection, MentionProjects, NewTerminal, NewWindow, OpenSearchResults, OpenSettings, PasteClipboard, Quit,
  SaveFile, SearchProject, SplitDown, SplitRight, ToggleAgents, ToggleTerminal, ViewOnHost,
};

/// Starts the application and runs it until it quits.
pub fn run() {
  let app = gpui_kit::application().with_assets(Assets);
  app.on_reopen(|cx| {
    if cx.windows().is_empty() {
      vekro_ui::open_last_window(cx);
    }
    cx.activate(true);
  });
  app.run(move |cx| {
    gpui_kit::init(cx);
    cx.activate(true);
    cx.bind_keys([
      KeyBinding::new("cmd-q", Quit, None),
      KeyBinding::new("cmd-n", NewWindow, None),
      KeyBinding::new("cmd-,", OpenSettings, None),
      KeyBinding::new("cmd-t", NewTerminal, None),
      KeyBinding::new("cmd-s", SaveFile, None),
      KeyBinding::new("cmd-w", CloseTab, None),
      KeyBinding::new("cmd-j", ToggleTerminal, None),
      KeyBinding::new("cmd-shift-a", ToggleAgents, None),
      KeyBinding::new("cmd-shift-g", ViewOnHost, None),
      KeyBinding::new("cmd-shift-m", MentionProjects, None),
      KeyBinding::new("cmd-shift-f", SearchProject, None),
      KeyBinding::new("cmd-enter", OpenSearchResults, Some("ProjectSearch")),
      KeyBinding::new("cmd-d", SplitRight, None),
      KeyBinding::new("cmd-shift-d", SplitDown, None),
      // The terminal's own: they act on the selection and the screen of the shell that has the keyboard.
      KeyBinding::new("cmd-c", CopySelection, Some("Terminal")),
      KeyBinding::new("cmd-v", PasteClipboard, Some("Terminal")),
      KeyBinding::new("cmd-k", ClearBuffer, Some("Terminal")),
    ]);
    vekro_ui::init(cx);
    cx.on_action(|_: &Quit, cx| cx.quit());
    // File → New Window and the Profiles menu are handled by `vekro_ui::init`.
    vekro_ui::open_last_window(cx);
  });
}
