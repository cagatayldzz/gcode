//! The Vekro interface: projects, editor, terminal and Git in one window, built with GPUI Kit.

mod assets;
mod components;
mod editor;
mod explorer;
mod project_setup;
mod selection;
mod settings;
mod terminal;
mod theme;
mod workspace;

use gpui_kit::*;

pub use assets::Assets;
pub use workspace::{Workspace, TOOLBAR_HEIGHT};

actions!(vekro, [Quit, NewWindow, AddProject, OpenSettings, NewTerminal, SaveFile]);

/// Loads the bundled fonts.
pub fn init(cx: &mut App) {
  if let Err(error) = cx.text_system().add_fonts(vec![
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Geist.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono.ttf")),
  ]) {
    eprintln!("Could not load fonts: {error}");
  }
}
