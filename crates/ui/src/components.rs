//! Interface pieces used in more than one place.

use crate::{
  project_settings::icon_path,
  theme::{colors, project_color, FONT_SIZE},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::ButtonCustomVariant,
    clipboard::Clipboard,
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    scroll::{Scrollbar, ScrollbarHandle, ScrollbarMode, ScrollbarThumbStyle},
    skeleton::Skeleton,
    text::{TextView, TextViewStyle},
    v_flex, ActiveTheme as _, Icon,
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

/// The text size of a path in the Changes list.
pub(crate) const CHANGE_PATH_FONT_SIZE: Pixels = px(14.);

/// An estimate of the width of one character of a path in [`CHANGE_PATH_FONT_SIZE`].
const PATH_CHAR_WIDTH: f32 = 0.54 * 14.;

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

/// Escapes what Markdown would read as formatting, so that a path shows as it is written.
fn plain_markdown(text: &str) -> String {
  let mut escaped = String::with_capacity(text.len());
  for character in text.chars() {
    if "\\`*_{}[]<>()#+-.!|~".contains(character) {
      escaped.push('\\');
    }
    escaped.push(character);
  }
  escaped
}

/// A changed file's path above its diff: a copy button, then the path as text that can be selected with the mouse. A renamed file
/// shows where it was before, then an arrow. `id` tells the path bars of different places apart.
pub(crate) fn file_path_bar(id: &str, path: &str, from: Option<&str>, cx: &App) -> impl IntoElement {
  let text = match from {
    Some(from) => format!("{} → {}", plain_markdown(from), plain_markdown(path)),
    None => plain_markdown(path),
  };
  h_flex()
    .min_w_0()
    .gap_2()
    .items_center()
    .child(
      div()
        .flex_none()
        .cursor_pointer()
        .child(Clipboard::new(SharedString::from(format!("copy-{id}-path"))).value(path.to_string()).tooltip("Copy relative path")),
    )
    .child(
      TextView::markdown(SharedString::from(format!("{id}-path")), text)
        .selectable(true)
        .style(TextViewStyle { paragraph_gap: rems(0.), ..Default::default() })
        .min_w_0()
        .text_size(crate::theme::FONT_SIZE_SM)
        .text_color(cx.theme().foreground),
    )
}

/// The thickness of the thumb of a slim scrollbar, and how far it stands from the edge.
const SLIM_THUMB_WIDTH: Pixels = px(6.);
const SLIM_THUMB_INSET: Pixels = px(2.);

/// A vertical scrollbar in the style of the system's overlay ones: a thin, round, translucent thumb with no track, which shows while
/// the list scrolls and fades after. Put it over the list in a `relative` parent, in a `div().absolute().inset_0()`.
pub(crate) fn slim_scrollbar<H: ScrollbarHandle + Clone>(handle: &H, id: impl Into<ElementId>, cx: &App) -> Scrollbar {
  let thumb = colors(cx).text_secondary;
  Scrollbar::vertical(handle).id(id).mode(ScrollbarMode::Scrolling).viewport_from_layout().styles(move |styles| {
    styles
      .thumb(|style| style.bg(thumb.opacity(0.45)).width(SLIM_THUMB_WIDTH).inset(SLIM_THUMB_INSET).radius(SLIM_THUMB_WIDTH))
      .thumb_hover(|style| style.bg(thumb.opacity(0.65)).width(SLIM_THUMB_WIDTH).inset(SLIM_THUMB_INSET).radius(SLIM_THUMB_WIDTH))
      .thumb_active(|style| style.bg(thumb.opacity(0.8)).width(SLIM_THUMB_WIDTH).inset(SLIM_THUMB_INSET).radius(SLIM_THUMB_WIDTH))
  })
}

/// The color of a changed file's mark: green for new, red for deleted, amber for modified and so on.
pub(crate) fn change_kind_icon(kind: ws_git::ChangeKind) -> &'static str {
  match kind {
    ws_git::ChangeKind::New => "icons/square-plus.svg",
    ws_git::ChangeKind::Modified => "icons/square-dot.svg",
    ws_git::ChangeKind::Deleted => "icons/square-minus.svg",
    ws_git::ChangeKind::Renamed => "icons/square-arrow-right.svg",
    ws_git::ChangeKind::Conflicted => "icons/triangle-alert.svg",
  }
}

pub(crate) fn kind_color(kind: ws_git::ChangeKind, palette: &crate::theme::Colors) -> Hsla {
  match kind {
    ws_git::ChangeKind::New => palette.new,
    ws_git::ChangeKind::Modified => palette.modified,
    ws_git::ChangeKind::Deleted => palette.deleted,
    ws_git::ChangeKind::Renamed => palette.renamed,
    ws_git::ChangeKind::Conflicted => palette.conflicted,
  }
}

/// The mark of a kind of change, as in the Changes list: a square with a plus for a new file, a dot for a modified one and so on.
pub(crate) fn change_kind_mark(kind: ws_git::ChangeKind, cx: &App) -> Icon {
  Icon::empty().path(change_kind_icon(kind)).size_4().text_color(kind_color(kind, &colors(cx)))
}

/// A path as the Changes list shows it: the folders in the secondary color, the file name after them in the text color.
pub(crate) fn path_label(path: &str, cx: &App) -> Div {
  let (folder, name) = match path.rsplit_once('/') {
    Some((folder, name)) => (Some(format!("{folder}/")), name),
    None => (None, path),
  };
  h_flex()
    .flex_none()
    .whitespace_nowrap()
    .text_size(CHANGE_PATH_FONT_SIZE)
    .font_weight(FontWeight::NORMAL)
    .children(folder.map(|folder| div().text_color(colors(cx).text_secondary).child(folder)))
    .child(name.to_string())
}

/// The strip of a scrollbar that stays in view: a column of its own beside a list, a shade darker than the list, with a thin round thumb,
/// so that it never covers what the list shows. Put it after the list in a row; the list is the part with `flex_1`.
pub(crate) fn scrollbar_strip<H: ScrollbarHandle + Clone>(handle: &H, id: impl Into<ElementId>, cx: &App) -> Div {
  let palette = colors(cx);
  div().relative().flex_none().w(STRIP_WIDTH).h_full().bg(palette.surface).border_l_1().border_color(palette.border).child(
    Scrollbar::vertical(handle).id(id).mode(ScrollbarMode::Always).viewport_from_layout().styles(|styles| {
      let thumb = |style: ScrollbarThumbStyle| style.width(STRIP_THUMB_WIDTH).inset(STRIP_THUMB_INSET).radius(STRIP_THUMB_WIDTH);
      styles
        .track(|track| track.width(STRIP_WIDTH))
        .track_hover(|track| track.width(STRIP_WIDTH))
        .track_active(|track| track.width(STRIP_WIDTH))
        .thumb(thumb)
        .thumb_hover(thumb)
        .thumb_active(thumb)
    }),
  )
}

/// The strip of [`scrollbar_strip`], and the thumb in it: a little narrower than the Kit's 16 pixels.
const STRIP_WIDTH: Pixels = px(13.);
const STRIP_THUMB_WIDTH: Pixels = px(6.);
const STRIP_THUMB_INSET: Pixels = px(3.);
