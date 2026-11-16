//! The Vekro interface: projects, editor, terminal and Git in one window, built with GPUI Kit.

mod assets;
mod explorer;
mod workspace;

use gpui_kit::{
  component::{Theme, ThemeMode},
  *,
};

pub use assets::Assets;
pub use workspace::{Workspace, TOOLBAR_HEIGHT};

actions!(vekro, [Quit, NewWindow, AddProject]);

/// Loads the interface fonts and applies the dark theme the design uses.
pub fn init(cx: &mut App) {
  Theme::change(ThemeMode::Dark, None, cx);
  Theme::update(cx, |theme| {
    let colors = &mut theme.colors;
    colors.background = rgb(0x0f1217).into();
    colors.foreground = rgb(0xe6e8ec).into();
    colors.sidebar = rgb(0x12161c).into();
    colors.sidebar_foreground = rgb(0xe6e8ec).into();
    colors.sidebar_border = rgb(0x232831).into();
    colors.sidebar_accent = rgb(0x1d2430).into();
    colors.sidebar_accent_foreground = rgb(0xe6e8ec).into();
    colors.border = rgb(0x232831).into();
    colors.muted_foreground = rgb(0x9aa3b2).into();
    colors.primary = rgb(0x6c9be0).into();
  });
  if let Err(error) = cx.text_system().add_fonts(vec![
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Geist.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono.ttf")),
  ]) {
    eprintln!("Could not load fonts: {error}");
  }
}
