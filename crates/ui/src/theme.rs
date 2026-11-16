//! Light and dark colors, following GitHub Desktop, switched with the system appearance.

use gpui_kit::{
  component::{ActiveTheme as _, Theme, ThemeMode},
  *,
};
use vekro_storage::ThemeChoice;

/// The colors of one appearance.
struct Palette {
  background: u32,
  foreground: u32,
  sidebar: u32,
  border: u32,
  muted: u32,
  muted_foreground: u32,
  hover: u32,
  primary: u32,
  popover: u32,
  danger: u32,
}

const DARK: Palette = Palette {
  background: 0x24292e,
  foreground: 0xe1e4e8,
  sidebar: 0x1f2428,
  border: 0x1b1f23,
  muted: 0x2f363d,
  muted_foreground: 0x959da5,
  hover: 0x2b3036,
  primary: 0x0366d6,
  popover: 0x2f363d,
  danger: 0xf97583,
};

const LIGHT: Palette = Palette {
  background: 0xffffff,
  foreground: 0x24292e,
  sidebar: 0xf6f8fa,
  border: 0xe1e4e8,
  muted: 0xf1f3f5,
  muted_foreground: 0x586069,
  hover: 0xeaecef,
  primary: 0x0366d6,
  popover: 0xffffff,
  danger: 0xcb2431,
};

/// Applies the chosen appearance (the system's, unless the user picked one) and paints the window with GitHub Desktop's colors.
pub(crate) fn apply_theme(choice: ThemeChoice, window: &mut Window, cx: &mut App) {
  match choice {
    ThemeChoice::System => Theme::sync_system_appearance(Some(window), cx),
    ThemeChoice::Light => Theme::change(ThemeMode::Light, Some(window), cx),
    ThemeChoice::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
  }
  let palette = if cx.theme().is_dark() { &DARK } else { &LIGHT };
  Theme::update(cx, |theme| {
    let colors = &mut theme.colors;
    colors.background = rgb(palette.background).into();
    colors.foreground = rgb(palette.foreground).into();
    colors.sidebar = rgb(palette.sidebar).into();
    colors.sidebar_foreground = rgb(palette.foreground).into();
    colors.sidebar_border = rgb(palette.border).into();
    colors.sidebar_accent = rgb(palette.hover).into();
    colors.sidebar_accent_foreground = rgb(palette.foreground).into();
    colors.sidebar_primary = rgb(palette.primary).into();
    colors.sidebar_primary_foreground = rgb(0xffffff).into();
    colors.border = rgb(palette.border).into();
    colors.input = rgb(palette.border).into();
    colors.popover = rgb(palette.popover).into();
    colors.popover_foreground = rgb(palette.foreground).into();
    colors.muted = rgb(palette.muted).into();
    colors.muted_foreground = rgb(palette.muted_foreground).into();
    colors.secondary = rgb(palette.muted).into();
    colors.secondary_foreground = rgb(palette.foreground).into();
    colors.accent = rgb(palette.hover).into();
    colors.accent_foreground = rgb(palette.foreground).into();
    colors.primary = rgb(palette.primary).into();
    colors.primary_foreground = rgb(0xffffff).into();
    colors.ring = rgb(palette.primary).into();
    colors.selection = rgb(palette.primary).opacity(0.35).into();
    colors.danger = rgb(palette.danger).into();
  });
  window.refresh();
}
