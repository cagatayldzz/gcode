//! The Vekro interface: projects, editor, terminal and Git in one window, built with GPUI Kit.

mod assets;
mod components;
mod explorer;
mod panels;
mod project_setup;
mod settings;
mod theme;
mod workspace;

use gpui_kit::*;

pub use assets::Assets;
pub use workspace::{Workspace, TOOLBAR_HEIGHT};

actions!(vekro, [Quit, NewWindow, AddProject, OpenSettings]);

/// Loads the interface fonts.
pub fn init(cx: &mut App) {
  if let Err(error) = cx.text_system().add_fonts(vec![
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Geist.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono.ttf")),
  ]) {
    eprintln!("Could not load fonts: {error}");
  }
}
