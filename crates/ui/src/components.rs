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
    skeleton::Skeleton,
    v_flex, ActiveTheme as _,
  },
  *,
};
use ws_storage::Project;

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

pub(crate) use crate::labels::{size_label, time_ago};

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

/// One tab of a tab strip (editor files, terminals): a divider on its right; the shown tab uses the editor background with an accent line on top.
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
    .when(shown, |tab| tab.bg(c.background).text_color(c.text).child(div().absolute().top_0().left_0().right_0().h_0p5().bg(c.tab_indicator)))
    .when(!shown, |tab| tab.text_color(c.text_secondary).hover(|tab| tab.bg(c.hover)))
    .child(label)
    .child(close)
}

/// The strip that holds the tabs: gray, with a line under it.
pub(crate) fn strip(cx: &App) -> Div {
  let c = colors(cx);
  h_flex().h_9().flex_none().bg(c.surface).border_b_1().border_color(c.border)
}

/// A bar of a skeleton: a quiet placeholder for text that is being read. `width` is a share of the line, in percent.
fn skeleton_bar(width: f32, height: Pixels) -> impl IntoElement {
  Skeleton::new().w(relative(width / 100.)).h(height).rounded_md()
}

/// Placeholder rows for a list of two-line entries (commits, files), shown while what the list holds is being read.
pub(crate) fn skeleton_list(rows: usize) -> AnyElement {
  v_flex()
    .size_full()
    .gap_3()
    .px_3()
    .py_2()
    .children((0..rows).map(|row| {
      // The widths vary a little, as text does.
      let (first, second) = [(86., 48.), (72., 38.), (92., 52.), (64., 44.)][row % 4];
      v_flex().gap_1p5().child(skeleton_bar(first, px(12.))).child(skeleton_bar(second, px(10.)))
    }))
    .into_any_element()
}

/// A placeholder for the main area of a commit or a change: its title, its files and a body of lines.
pub(crate) fn skeleton_page() -> AnyElement {
  v_flex()
    .size_full()
    .child(v_flex().gap_2().p_4().child(skeleton_bar(55., px(16.))).child(skeleton_bar(30., px(12.))))
    .child(h_flex().flex_1().min_h_0().child(div().w_64().flex_none().h_full().child(skeleton_list(10))).child(skeleton_lines(16)))
    .into_any_element()
}

/// Placeholder lines for the body of a diff or a file that is being read.
pub(crate) fn skeleton_lines(count: usize) -> AnyElement {
  v_flex()
    .flex_1()
    .min_w_0()
    .gap_2()
    .p_4()
    .children((0..count).map(|line| skeleton_bar([90., 70., 80., 45., 60., 85.][line % 6], px(12.))))
    .into_any_element()
}

/// The text size of a path in the Changes list: one step below the app's text size.
pub(crate) const CHANGE_PATH_FONT_SIZE: Pixels = px(13.);

/// An estimate of the width of one character of a path in [`CHANGE_PATH_FONT_SIZE`].
const PATH_CHAR_WIDTH: f32 = 0.54 * 13.;

/// `path` cut to fit `room` pixels: the file name is always kept whole and the end of the folder gives way to `…`
/// (`app/(page)/finan…/page.client.tsx`), the way a path is shortened in a long file list. A zero `room` (before the first layout)
/// leaves the path as it is.
pub(crate) fn shortened_path(path: &str, room: f32) -> String {
  let fits = (room / PATH_CHAR_WIDTH) as usize;
  if room <= 0. || path.chars().count() <= fits {
    return path.to_string();
  }
  let Some((folder, name)) = path.rsplit_once('/') else {
    return path.to_string();
  };
  let kept = fits.saturating_sub(name.chars().count() + 2).max(1);
  format!("{}…/{name}", folder.chars().take(kept).collect::<String>())
}

/// `path` cut to fit `room` pixels by dropping the middle (`src/co…er.tsx`), keeping the start and the end of it: for a path shown
/// beside another, such as the two of a renamed file.
pub(crate) fn shortened_middle(path: &str, room: f32) -> String {
  let fits = (room / PATH_CHAR_WIDTH) as usize;
  let chars: Vec<char> = path.chars().collect();
  if room <= 0. || chars.len() <= fits || fits < 3 {
    return path.to_string();
  }
  let tail = (fits - 1) / 2;
  let head = fits - 1 - tail;
  format!("{}…{}", chars[..head].iter().collect::<String>(), chars[chars.len() - tail..].iter().collect::<String>())
}
