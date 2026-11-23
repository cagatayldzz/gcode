//! The application: startup, windows and key bindings. `apps/desktop` only calls [`run`].

use gpui_kit::*;
use ws_ui::{
  AddLineComment, Assets, ClearBuffer, CloseTab, CopySelection, FindInFile, GoToFile, MentionProjects, NewTerminal, OpenCommandPalette,
  OpenSearchResults, OpenSettings, PasteClipboard, Quit, RemoveLineComment, ReplaceInFile, SaveFile, SearchProject, SelectNextOccurrence, SplitDown,
  SplitRight, ToggleAgents, ToggleLineComment, ToggleTerminal, ViewOnHost,
};

/// Starts the application and runs it until it quits.
pub fn run() {
  let app = gpui_kit::application().with_assets(Assets);
  app.on_reopen(|cx| {
    if cx.windows().is_empty() {
      ws_ui::open_last_window(cx);
    }
    cx.activate(true);
  });
  app.run(move |cx| {
    gpui_kit::init(cx);
    cx.activate(true);
    cx.bind_keys([
      KeyBinding::new("cmd-q", Quit, None),
      KeyBinding::new("cmd-,", OpenSettings, None),
      KeyBinding::new("cmd-t", NewTerminal, None),
      KeyBinding::new("cmd-p", GoToFile, None),
      KeyBinding::new("cmd-shift-p", OpenCommandPalette, None),
      KeyBinding::new("cmd-s", SaveFile, None),
      KeyBinding::new("cmd-k cmd-c", AddLineComment, Some("Input && mode == editor")),
      KeyBinding::new("cmd-k cmd-u", RemoveLineComment, Some("Input && mode == editor")),
      KeyBinding::new("cmd-f", FindInFile, Some("Input && mode == editor")),
      KeyBinding::new("cmd-alt-f", ReplaceInFile, Some("Input && mode == editor")),
      KeyBinding::new("cmd-shift-f", SearchProject, Some("Input && mode == editor")),
      KeyBinding::new("cmd-d", SelectNextOccurrence, Some("Input && mode == editor")),
      KeyBinding::new("cmd-/", ToggleLineComment, Some("Input && mode == editor")),
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
    ws_ui::init(cx);
    cx.on_action(|_: &Quit, cx| cx.quit());
    // The Profiles menu is handled by `ws_ui::init`.
    ws_ui::open_last_window(cx);
  });
}
