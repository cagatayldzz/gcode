//! Interface pieces used in more than one place.

use crate::{
  project_settings::icon_path,
  theme::{colors, project_color},
};
use gpui_kit::{
  component::{button::ButtonCustomVariant, ActiveTheme as _},
  *,
};
use vekro_storage::Project;

/// The blue fill of a chosen avatar, tab or option. The kit's own primary style inverts the text color,
/// which is black on white in a dark theme.
pub(crate) fn selected_style(cx: &App) -> ButtonCustomVariant {
  let primary = cx.theme().primary;
  ButtonCustomVariant::new(cx).color(primary).foreground(colors(cx).on_accent).hover(primary).active(primary)
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

/// How a project's avatar is drawn: the repository menu and the project list in Settings share it.
pub(crate) struct AvatarLook {
  pub fill: Option<Hsla>,
  pub text: Hsla,
  /// The asset path of the chosen icon; the first letter is drawn when there is none.
  pub icon: Option<SharedString>,
  pub letter: String,
}

/// The look of a project's avatar. A chosen color tints it, and fills it while the project is open; without a color
/// the theme's blue marks the open project.
pub(crate) fn avatar_look(project: &Project, selected: bool, cx: &App) -> AvatarLook {
  let palette = colors(cx);
  let primary = cx.theme().primary;
  let color = project.color.as_deref().and_then(project_color);
  let (fill, text) = match (selected, color) {
    (true, Some(color)) => (Some(color), white()),
    (true, None) => (Some(primary), palette.on_accent),
    (false, Some(color)) => (Some(color.opacity(0.2)), color),
    (false, None) => (None, palette.rail_text),
  };
  AvatarLook {
    fill,
    text,
    icon: project.icon.as_deref().and_then(icon_path),
    letter: project.name().chars().take(1).flat_map(char::to_uppercase).collect(),
  }
}
