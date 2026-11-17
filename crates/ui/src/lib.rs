//! The Vekro interface: projects, editor, terminal and Git in one window, built with GPUI Kit.

mod assets;
mod close_order;
mod commit_settings;
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
mod theme;
mod workspace;

use gpui_kit::*;

pub use assets::Assets;
pub use workspace::{Workspace, TOOLBAR_HEIGHT};

actions!(vekro, [Quit, NewWindow, AddProject, OpenSettings, NewTerminal, SaveFile, CloseTab, ToggleTerminal]);

/// Loads the bundled fonts.
pub fn init(cx: &mut App) {
  if let Err(error) = cx.text_system().add_fonts(vec![
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Geist.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono.ttf")),
  ]) {
    eprintln!("Could not load fonts: {error}");
  }
}
