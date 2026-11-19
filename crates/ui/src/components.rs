//! Interface pieces used in more than one place.

use crate::{
  project_settings::icon_path,
  theme::{colors, project_color, FONT_SIZE},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::ButtonCustomVariant,
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    ActiveTheme as _,
  },
  *,
};
use vekro_storage::Project;

/// The blue fill of a chosen avatar, tab or option. The kit's own primary style inverts the text color,
/// which is black on white in a dark theme.
pub(crate) fn selected_style(cx: &App) -> ButtonCustomVariant {
  let primary = cx.theme().primary;
  ButtonCustomVariant::new(cx).color(primary).foreground(colors(cx).on_accent).hover(primary).active(primary)
}

/// A text field with the soft border the dialogs use; the kit's own `theme.input` border is as strong as a checkbox's.
pub(crate) fn soft_input(state: &Entity<InputState>, cx: &App) -> Input {
  Input::new(state).border_color(colors(cx).border)
}

/// A multi-line field with the same soft border.
pub(crate) fn soft_textarea(state: &Entity<TextareaState>, cx: &App) -> Textarea {
  Textarea::new(state).border_color(colors(cx).border)
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

/// One tab of a tab strip (editor files, terminals): a divider on its right; the shown tab is white with an accent line on top.
/// The caller adds the click handler.
pub(crate) fn strip_tab(id: SharedString, label: impl IntoElement, shown: bool, close: impl IntoElement, cx: &App) -> Stateful<Div> {
  let c = colors(cx);
  h_flex()
    .id(id)
    .relative()
    .h_full()
    .px_2()
    .gap_2()
    .items_center()
    .border_r_1()
    .border_color(c.border)
    .text_size(FONT_SIZE)
    .cursor_pointer()
    .when(shown, |tab| tab.bg(c.background).text_color(c.text).child(div().absolute().top_0().left_0().right_0().h_0p5().bg(c.accent)))
    .when(!shown, |tab| tab.text_color(c.text_secondary).hover(|tab| tab.bg(c.hover)))
    .child(label)
    .child(close)
}

/// The strip that holds the tabs: gray, with a line under it.
pub(crate) fn strip(cx: &App) -> Div {
  let c = colors(cx);
  h_flex().h_9().flex_none().bg(c.surface_alt).border_b_1().border_color(c.border)
}
