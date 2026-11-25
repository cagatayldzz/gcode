//! Free helpers shared by the workspace views.

use super::*;

/// A small terminal icon that blinks while a command runs in the project's terminal, shown on the project's tab.
pub(super) fn terminal_activity_icon(cx: &App) -> impl IntoElement {
  div().flex_none().child(Icon::empty().path("icons/terminal.svg").size_3p5().text_color(colors(cx).text_secondary)).with_animation(
    "terminal-activity",
    Animation::new(TERMINAL_BLINK).repeat().with_easing(pulsating_between(0.25, 1.)),
    |icon, delta| icon.opacity(delta),
  )
}

pub(super) fn project_bar_button(id: impl Into<ElementId>, selected: bool, cx: &App) -> Button {
  let palette = colors(cx);
  let hover = if selected { palette.project_tab_selected } else { palette.rail_hover };
  Button::new(id)
    .custom(
      ButtonCustomVariant::new(cx)
        .color(palette.rail.opacity(0.))
        .foreground(if selected { palette.explorer_selected_text } else { palette.text })
        .hover(hover)
        .active(hover),
    )
    .when(selected, |button| button.bg(palette.project_tab_selected))
    .cursor_pointer()
    .border_0()
    .h_8()
    .max_w_56()
    .rounded_md()
}

/// What is waiting in a project, for its row in the project menu and its tab in the title bar: commits to push (an arrow), and
/// files with changes (a dot). `on_accent` draws them for the open project's tab, which is filled with the accent color.
pub(super) fn project_status_marks(scope: &str, path: &Path, status: Option<&ProjectStatus>, on_accent: bool, cx: &App) -> Vec<AnyElement> {
  let palette = colors(cx);
  let (push, changed) = status.map_or((false, 0), |status| (status.push, status.changed));
  let dot = if on_accent { palette.on_accent } else { cx.theme().primary };
  let arrow = if on_accent { palette.on_accent } else { cx.theme().muted_foreground };
  let mut marks = Vec::new();
  if push {
    marks.push(
      div()
        .id(SharedString::from(format!("{scope}-push-{}", path.display())))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .when(!on_accent, |mark| mark.px_2().rounded_full().bg(palette.hover))
        .tooltip(|window, cx| Tooltip::new("Commits waiting to be pushed").build(window, cx))
        .child(Icon::empty().path("icons/arrow-up.svg").size_3p5().text_color(arrow))
        .into_any_element(),
    );
  }
  if changed > 0 {
    marks.push(
      div()
        .id(SharedString::from(format!("{scope}-changes-{}", path.display())))
        .flex_none()
        .size_2()
        .rounded_full()
        .bg(dot)
        .tooltip(move |window, cx| {
          Tooltip::new(if changed == 1 { "1 uncommitted change".to_string() } else { format!("{changed} uncommitted changes") }).build(window, cx)
        })
        .into_any_element(),
    );
  }
  marks
}

/// One project in a list (a category's list, the repository menu): its avatar and name; a click opens it and closes the list,
/// a right click opens the project's menu.
pub(super) fn project_list_row(
  workspace: &Entity<Workspace>,
  project: &Project,
  selected: bool,
  status: Option<&ProjectStatus>,
  cx: &App,
) -> AnyElement {
  let palette = colors(cx);
  let look = avatar_look(project, selected, cx);
  let (path, name) = (project.path.clone(), SharedString::from(project.name()));
  let (favorite, star_workspace, star_path) = (project.favorite, workspace.clone(), project.path.clone());
  let muted = cx.theme().muted_foreground;
  h_flex()
    .id(SharedString::from(format!("project-row-{}", path.display())))
    .group("project-row")
    .gap_2()
    .px_2()
    .py_1()
    .rounded_md()
    .items_center()
    .cursor_pointer()
    .when(selected, |row| row.bg(palette.selected))
    .hover(|row| row.bg(palette.hover))
    // The star, at the left: filled and always shown for a favorite, an outline that appears while the pointer is on the row otherwise.
    .child(
      div()
        .id(SharedString::from(format!("project-star-{}", path.display())))
        .size_6()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .hover(|star| star.bg(palette.hover))
        .when(!favorite, |star| star.opacity(0.5))
        .tooltip(move |window, cx| Tooltip::new(if favorite { "Remove from favorites" } else { "Add to favorites" }).build(window, cx))
        .child(Icon::empty().path(if favorite { "icons/star-fill.svg" } else { "icons/star.svg" }).size_4().text_color(if favorite {
          palette.modified
        } else {
          muted
        }))
        .on_click(move |_, _, cx| {
          // The click must not also open the project.
          cx.stop_propagation();
          star_workspace.update(cx, |this, cx| this.toggle_favorite(&star_path, cx));
        }),
    )
    .child(
      div()
        .size_6()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_sm()
        .text_color(look.text)
        .when_some(look.fill, |avatar, fill| avatar.bg(fill))
        .when_some(look.icon.clone(), |avatar, icon| avatar.child(Icon::empty().path(icon).size_4()))
        .when(look.icon.is_none(), |avatar| avatar.child(look.letter.clone())),
    )
    .child(Label::new(name).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
    .children(project_status_marks("list", &path, status, false, cx))
    .on_click({
      let workspace = workspace.clone();
      move |_, window, cx| {
        workspace.update(cx, |this, cx| {
          this.projects_menu_open = false;
          this.select_project(path.clone(), window, cx);
        })
      }
    })
    .into_any_element()
}

/// What a failed push, pull or fetch says about the account's token, if it says anything: the host turned the token down, or
/// accepted it but refused commits that change workflow files.
pub(super) fn refusal_kind(message: &str) -> Option<IssueKind> {
  if message.contains(ws_git::WORKFLOWS_REFUSED) {
    Some(IssueKind::Workflows)
  } else if message.contains(ws_git::TOKEN_REFUSED) {
    Some(IssueKind::Push)
  } else {
    None
  }
}

/// Whether a failed push, pull or fetch is fixed by connecting the account again: there was no login to give Git, or the host
/// turned the account's token down.
pub(super) fn needs_reconnect(message: &str) -> bool {
  message.contains(ws_git::NO_LOGIN) || message.contains(ws_git::TOKEN_REFUSED)
}

/// How long ago a commit was made, in words: "2 minutes ago", "3 hours ago", "yesterday".
pub(super) fn ago_long(seconds_since_epoch: i64) -> String {
  let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs() as i64);
  let seconds = (now - seconds_since_epoch).max(0);
  let plural = |count: i64, unit: &str| if count == 1 { format!("1 {unit} ago") } else { format!("{count} {unit}s ago") };
  match seconds {
    0..=59 => "just now".to_string(),
    60..=3599 => plural(seconds / 60, "minute"),
    3600..=86399 => plural(seconds / 3600, "hour"),
    86400..=172799 => "yesterday".to_string(),
    172800..=2_591_999 => plural(seconds / 86400, "day"),
    2_592_000..=31_535_999 => plural(seconds / 2_592_000, "month"),
    _ => plural(seconds / 31_536_000, "year"),
  }
}

pub(super) use crate::components::time_ago;

/// What a category is called in a menu or a tooltip: its name, or its number while it has none.
pub(super) fn category_label(index: usize, category: &Category) -> String {
  if category.name.is_empty() {
    format!("Category {}", index + 1)
  } else {
    category.name.clone()
  }
}

/// A square icon button for the title bar, as tall as the repository switcher. The Kit's `Button` fixes its icon at a small
/// size, so this draws the icon itself at `size_5`.
/// A small blue circle with a count (99+ above that), for the files with changes.
pub(super) fn status_badge(cx: &App) -> Div {
  let palette = colors(cx);
  div().min_w_4().h_4().flex().flex_none().items_center().justify_center().rounded_full().bg(cx.theme().primary).text_color(palette.on_accent)
}

/// A plain green dot for a button that has something running but nothing to count. The count badge is blue, so the two never
/// read as the same signal; the ring in the rail's color separates the dot from the icon under it.
pub(super) fn status_dot(cx: &App) -> Div {
  let palette = colors(cx);
  div().size_3().flex_none().rounded_full().bg(palette.new).border_2().border_color(palette.rail)
}

pub(super) fn count_badge(count: usize, cx: &App) -> Div {
  status_badge(cx).px_1().text_size(BADGE_FONT_SIZE).child(if count > 99 { "99+".to_string() } else { count.to_string() })
}

pub(super) fn explorer_file_icon(path: &Path, palette: &crate::theme::Colors) -> Icon {
  let (icon, color) = match path.file_name().and_then(|name| name.to_str()) {
    Some(".editorconfig") => ("icons/settings.svg", palette.text_secondary),
    Some(".gitignore") => ("icons/git-branch.svg", palette.text_secondary),
    Some("README.md") => ("icons/info.svg", palette.link),
    Some("bun.lock") => ("icons/braces.svg", palette.modified),
    Some("Cargo.lock") => ("icons/list.svg", palette.text_secondary),
    _ => match crate::editor::language_for(path) {
      "toml" => ("icons/settings.svg", palette.text_secondary),
      "json" => ("icons/braces.svg", palette.modified),
      "rust" => ("icons/file-code.svg", palette.plist_icon),
      "markdown" => ("icons/arrow-down-to-line.svg", palette.link),
      "javascript" => ("icons/code.svg", palette.modified),
      "typescript" | "tsx" => ("icons/file-braces.svg", palette.link),
      "bash" => ("icons/terminal.svg", palette.text_secondary),
      "html" => ("icons/code-xml.svg", palette.plist_icon),
      "css" => ("icons/palette.svg", palette.link),
      "python" => ("icons/file-code-corner.svg", palette.link),
      "swift" => ("icons/feather.svg", palette.plist_icon),
      "go" => ("icons/file-code.svg", palette.link),
      "yaml" => ("icons/list-tree.svg", palette.modified),
      _ if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("plist")) => ("icons/rss.svg", palette.plist_icon),
      _ => ("icons/file-text.svg", palette.text_secondary),
    },
  };
  Icon::empty().path(icon).size_3().text_color(color)
}

pub(super) fn rail_button(
  id: &'static str,
  icon: Icon,
  tooltip: &'static str,
  selected: bool,
  large: bool,
  palette: &crate::theme::Colors,
  on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
  let hover = palette.rail_hover;
  div()
    .id(id)
    .role(gpui_kit::accesskit::Role::Button)
    .aria_label(tooltip)
    .map(|button| if large { button.size_10() } else { button.size_8() })
    .flex()
    .flex_none()
    .items_center()
    .justify_center()
    .rounded_md()
    .cursor_pointer()
    .text_color(palette.rail_text)
    .when(selected, |button| button.bg(hover))
    .hover(move |button| button.bg(hover))
    .child(icon.size_5())
    .tooltip(move |window, cx| Tooltip::new(tooltip).build(window, cx))
    .on_click(on_click)
}

/// The color of a changed file's mark: green for new, red for deleted, amber for modified and so on.
pub(super) fn change_kind_icon(kind: ws_git::ChangeKind) -> &'static str {
  match kind {
    ws_git::ChangeKind::New => "icons/square-plus.svg",
    ws_git::ChangeKind::Modified => "icons/square-dot.svg",
    ws_git::ChangeKind::Deleted => "icons/square-minus.svg",
    ws_git::ChangeKind::Renamed => "icons/square-arrow-right.svg",
    ws_git::ChangeKind::Conflicted => "icons/file-exclamation-point.svg",
  }
}

pub(super) fn kind_color(kind: ws_git::ChangeKind, palette: &crate::theme::Colors) -> Hsla {
  match kind {
    ws_git::ChangeKind::New => palette.new,
    ws_git::ChangeKind::Modified => palette.modified,
    ws_git::ChangeKind::Deleted => palette.deleted,
    ws_git::ChangeKind::Renamed => palette.renamed,
    ws_git::ChangeKind::Conflicted => palette.conflicted,
  }
}

/// One entry of a shortcut bar: what a command does, with the keys that run it as the keymap has them now. An unbound command
/// shows no keys.
pub(super) fn shortcut_hint(label: &'static str, action: &dyn Action, window: &Window) -> AnyElement {
  h_flex()
    .gap_1p5()
    .items_center()
    .flex_none()
    .child(label)
    .children(Kbd::binding_for_action(action, Some("Terminal"), window).map(|key| key.appearance(true)))
    .into_any_element()
}

/// The info bar under the terminals and the AI view: the shortcuts that work there.
pub(super) fn shortcut_bar(hints: Vec<AnyElement>, cx: &App) -> StatusBar {
  let palette = colors(cx);
  StatusBar::new()
    .bg(palette.surface_alt)
    .border_color(palette.border)
    .text_color(palette.text_secondary)
    .left(h_flex().items_center().gap_3().px_1().children(hints))
}

/// How far one step of a push, pull or fetch has got, from the percentage in git's progress line: the steps weigh differently
/// ("Receiving objects: 45% (20/46)" is the middle of a fetch, "Resolving deltas" its end).
pub(super) fn progress_fraction(line: &str) -> Option<f32> {
  let line = line.rsplit(['\r', '\n']).find(|part| part.contains('%')).unwrap_or(line);
  let percent: f32 = line.split('%').next()?.rsplit(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?;
  let line = line.to_lowercase();
  let (start, span) = if line.contains("resolving") {
    (0.8, 0.2)
  } else if line.contains("writing") {
    (0.4, 0.6)
  } else if line.contains("receiving") {
    (0.2, 0.6)
  } else if line.contains("compressing") {
    (0.2, 0.2)
  } else {
    (0., 0.2)
  };
  Some(start + span * (percent / 100.).clamp(0., 1.))
}
