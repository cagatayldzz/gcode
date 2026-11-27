//! The application: startup, windows and key bindings. `apps/desktop` only calls [`run`].

use gpui_kit::*;
use ws_ui::{
  AddLineComment, Assets, ClearBuffer, CloseTab, CopySelection, FindInFile, GoToFile, MentionProjects, NewTerminal, OpenCommandPalette,
  OpenSearchResults, OpenSettings, PasteClipboard, Quit, RemoveLineComment, ReplaceInFile, SaveFile, SearchProject, SelectNextOccurrence,
  SelectNextRow, SelectPreviousRow, SplitDown, SplitRight, ToggleAgents, ToggleLineComment, ToggleTerminal, ViewOnHost,
};

/// Starts the application and runs it until it quits.
pub fn run() {
  // A crash is written to the logs folder of the data directory before the process ends.
  ws_storage::install_panic_hook();
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
      KeyBinding::new("ctrl-.", gpui_kit::base::input::ToggleCodeActions, Some("Input && mode == editor")),
      KeyBinding::new("ctrl-space", gpui_kit::base::input::ShowCompletions, Some("Input && mode == editor")),
      KeyBinding::new("cmd-k cmd-c", AddLineComment, Some("Input && mode == editor")),
      KeyBinding::new("cmd-k cmd-u", RemoveLineComment, Some("Input && mode == editor")),
      KeyBinding::new("cmd-f", FindInFile, Some("Input && mode == editor")),
      KeyBinding::new("cmd-alt-f", ReplaceInFile, Some("Input && mode == editor")),
      KeyBinding::new("cmd-shift-f", SearchProject, Some("Input && mode == editor")),
      KeyBinding::new("cmd-d", SelectNextOccurrence, Some("Input && mode == editor")),
      KeyBinding::new("cmd-/", ToggleLineComment, Some("Input && mode == editor")),
      KeyBinding::new("cmd-w", CloseTab, None),
      // The Changes and History lists of the Git view, while one has the keyboard.
      KeyBinding::new("up", SelectPreviousRow, Some("ChangesList")),
      KeyBinding::new("down", SelectNextRow, Some("ChangesList")),
      KeyBinding::new("up", SelectPreviousRow, Some("HistoryList")),
      KeyBinding::new("down", SelectNextRow, Some("HistoryList")),
      KeyBinding::new("cmd-j", ToggleTerminal, None),
      KeyBinding::new("cmd-shift-a", ToggleAgents, None),
      KeyBinding::new("cmd-shift-g", ViewOnHost, None),
      KeyBinding::new("cmd-shift-m", MentionProjects, None),
      KeyBinding::new("cmd-shift-f", SearchProject, None),
      KeyBinding::new("cmd-enter", OpenSearchResults, Some("ProjectSearch")),
      // A binding without a context counts as the deepest match and would win over the editor's own ⌘D (add next occurrence), so
      // splitting is bound to the terminal only.
      KeyBinding::new("cmd-d", SplitRight, Some("Terminal")),
      KeyBinding::new("cmd-shift-d", SplitDown, None),
      // The terminal's own: they act on the selection and the screen of the shell that has the keyboard.
      KeyBinding::new("cmd-c", CopySelection, Some("Terminal")),
      // A diff table: copy the text or the lines chosen in it.
      KeyBinding::new("cmd-c", CopySelection, Some("DiffTable")),
      KeyBinding::new("cmd-v", PasteClipboard, Some("Terminal")),
      KeyBinding::new("cmd-k", ClearBuffer, Some("Terminal")),
    ]);
    ws_ui::init(cx);
    cx.on_action(|_: &Quit, cx| cx.defer(ws_ui::quit));
    // The Profiles menu is handled by `ws_ui::init`.
    ws_ui::open_last_window(cx);
  });
}
