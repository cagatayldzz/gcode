//! The Vekro interface: projects, editor, terminal and Git in one window, built with GPUI Kit.

mod assets;
mod change_view;
mod close_order;
mod commit_settings;
mod commit_view;
mod components;
mod connect_account;
mod connections;
mod editor;
mod explorer;
mod project_manager;
mod project_settings;
mod project_setup;
mod pull_request;
mod repository_picker;
mod selection;
mod settings;
mod terminal;
mod terminal_layout;
mod theme;
mod updates;
mod workspace;

use gpui_kit::*;

pub use assets::Assets;
pub use workspace::{Workspace, TOOLBAR_HEIGHT};

actions!(
  vekro,
  [
    Quit,
    NewWindow,
    AddProject,
    OpenSettings,
    NewTerminal,
    SaveFile,
    CloseTab,
    ToggleTerminal,
    // Terminal panes: split side by side, split one above the other.
    SplitRight,
    SplitDown,
    // In a terminal: copy the selection, paste, clear the history.
    CopySelection,
    PasteClipboard,
    ClearBuffer
  ]
);

/// Loads the bundled fonts.
pub fn init(cx: &mut App) {
  if let Err(error) = cx.text_system().add_fonts(vec![
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Geist.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono.ttf")),
  ]) {
    eprintln!("Could not load fonts: {error}");
  }
}
