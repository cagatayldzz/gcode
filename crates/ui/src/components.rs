//! Interface pieces used in more than one place.

use gpui_kit::{
  component::{button::ButtonCustomVariant, ActiveTheme as _},
  *,
};

/// The blue fill of a chosen avatar, tab or option. The kit's own primary style inverts the text color,
/// which is black on white in a dark theme.
pub(crate) fn selected_style(cx: &App) -> ButtonCustomVariant {
  let primary = cx.theme().primary;
  ButtonCustomVariant::new(cx).color(primary).foreground(white()).hover(primary).active(primary)
}

/// A path for display: the home folder is written as `~`.
pub(crate) fn display_path(path: &std::path::Path) -> String {
  let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
  match home.as_deref().and_then(|home| path.strip_prefix(home).ok()) {
    Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
    Some(rest) => format!("~/{}", rest.display()),
    None => path.display().to_string(),
  }
}
