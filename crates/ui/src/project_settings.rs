//! The project settings view: how a project is named and looks in the project menu (name, icon, color), where its Git remote
//! points, and removing it. It takes the main area like the terminals do, as one page with a bar of actions at the bottom.

use crate::{
  components::{avatar_look, display_path},
  project_setup::field,
  theme::{colors, project_color, FONT_SIZE, FONT_SIZE_SM, PROJECT_COLORS},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{InputEvent, InputState},
    label::Label,
    scroll::{Scrollbar, ScrollbarAxis, ScrollbarMode},
    v_flex, ActiveTheme, Disableable as _, Icon, StyledExt as _,
  },
  *,
};
use std::path::PathBuf;
use vekro_storage::Project;

/// The icons a project can take instead of its first letter.
pub(crate) const PROJECT_ICONS: [&str; 20] = [
  "rocket",
  "code",
  "terminal",
  "globe",
  "database",
  "server",
  "package",
  "zap",
  "flame",
  "leaf",
  "bug",
  "cpu",
  "gamepad-2",
  "music",
  "camera",
  "book-open",
  "heart",
  "star",
  "cloud",
  "shield",
];

/// The asset path of a project icon, if `name` is one of [`PROJECT_ICONS`].
pub(crate) fn icon_path(name: &str) -> Option<SharedString> {
  PROJECT_ICONS.contains(&name).then(|| SharedString::from(format!("icons/{name}.svg")))
}

/// The look the user chose, sent when Save is pressed.
pub(crate) struct AppearanceChosen {
  pub path: PathBuf,
  pub name: String,
  pub icon: Option<String>,
  pub color: Option<String>,
  /// The new remote address, only when the user changed it.
  pub remote: Option<String>,
  /// Also rename the project's folder on disk to the display name.
  pub rename_folder: bool,
}

pub(crate) enum ProjectSettingsEvent {
  Save(AppearanceChosen),
  RemoveRequested(PathBuf),
  /// Leave the view without saving.
  Close,
}

pub(crate) struct ProjectSettings {
  project: Project,
  input: Entity<InputState>,
  remote_input: Entity<InputState>,
  remote: String,
  rename_folder: bool,
  icon: Option<String>,
  color: Option<String>,
  scroll: ScrollHandle,
  _subscription: Subscription,
}

impl EventEmitter<ProjectSettingsEvent> for ProjectSettings {}

impl ProjectSettings {
  pub fn new(project: &Project, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let folder = project.folder_name();
    let current = project.name();
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(folder).default_value(current));
    let remote = vekro_git::remote_url(&project.path).unwrap_or_default();
    let remote_input = cx.new(|cx| InputState::new(window, cx).placeholder("https://github.com/owner/repository.git").default_value(remote.clone()));
    let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| match event {
      InputEvent::PressEnter { .. } => this.save(cx),
      InputEvent::Change => cx.notify(),
      _ => {}
    });
    Self {
      project: project.clone(),
      input,
      remote_input,
      remote,
      rename_folder: false,
      icon: project.icon.clone(),
      color: project.color.clone(),
      scroll: ScrollHandle::new(),
      _subscription: subscription,
    }
  }

  fn save(&self, cx: &mut Context<Self>) {
    let name = self.input.read(cx).value().to_string();
    let remote = self.remote_input.read(cx).value().trim().to_string();
    let remote = (remote != self.remote && !remote.is_empty()).then_some(remote);
    cx.emit(ProjectSettingsEvent::Save(AppearanceChosen {
      path: self.project.path.clone(),
      name,
      icon: self.icon.clone(),
      color: self.color.clone(),
      remote,
      rename_folder: self.rename_folder && self.folder_differs(cx),
    }));
  }

  /// Whether the display name is not the folder's name, so renaming the folder would change something.
  fn folder_differs(&self, cx: &Context<Self>) -> bool {
    let name = self.input.read(cx).value().trim().to_string();
    !name.is_empty() && name != self.project.folder_name()
  }

  /// Whether anything differs from what the project has now, which is what Save would change.
  fn is_dirty(&self, cx: &Context<Self>) -> bool {
    let name = self.input.read(cx).value().trim().to_string();
    let remote = self.remote_input.read(cx).value().trim().to_string();
    name != self.project.name()
      || self.icon != self.project.icon
      || self.color != self.project.color
      || (remote != self.remote && !remote.is_empty())
      || (self.rename_folder && self.folder_differs(cx))
  }

  fn swatch(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
    let (name, value) = PROJECT_COLORS[index];
    let chosen = self.color.as_deref() == Some(value);
    let palette = colors(cx);
    // A ring around the color marks the chosen one; the check keeps it readable on any color.
    div()
      .id(SharedString::from(format!("color-{name}")))
      .size_8()
      .flex()
      .items_center()
      .justify_center()
      .rounded_full()
      .border_2()
      .cursor_pointer()
      .border_color(if chosen { palette.text } else { gpui_kit::transparent_black() })
      .child(
        div()
          .size_5()
          .flex()
          .items_center()
          .justify_center()
          .rounded_full()
          .when_some(project_color(value), |swatch, color| swatch.bg(color))
          .when(chosen, |swatch| swatch.child(Icon::empty().path("icons/check.svg").size_3().text_color(white()))),
      )
      .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name).build(window, cx))
      .on_click(cx.listener(move |this, _, _, cx| {
        this.color = Some(value.into());
        cx.notify();
      }))
  }

  /// One square tile of the icon grid: the project's initial when `name` is `None`.
  fn icon_button(&self, name: Option<&'static str>, cx: &mut Context<Self>) -> impl IntoElement {
    let chosen = self.icon.as_deref() == name;
    let palette = colors(cx);
    let theme = cx.theme().clone();
    let tint = if chosen { palette.on_accent } else { theme.muted_foreground };
    div()
      .id(SharedString::from(format!("icon-{}", name.unwrap_or("letter"))))
      .size_9()
      .flex()
      .items_center()
      .justify_center()
      .rounded_md()
      .cursor_pointer()
      .when(chosen, |tile| tile.bg(palette.accent))
      .when(!chosen, |tile| tile.hover(|tile| tile.bg(palette.selected)))
      .text_color(tint)
      .map(|tile| match name {
        Some(name) => tile.child(Icon::empty().path(format!("icons/{name}.svg")).size_4()),
        None => tile.child(Label::new("Aa").text_size(FONT_SIZE_SM).font_semibold()),
      })
      .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name.unwrap_or("Project initial")).build(window, cx))
      .on_click(cx.listener(move |this, _, _, cx| {
        this.icon = name.map(str::to_owned);
        cx.notify();
      }))
  }

  /// The avatar, the name as typed and the folder: how the project looks in the project menu.
  fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let mut project = self.project.clone();
    let name = self.input.read(cx).value().trim().to_string();
    project.name = (!name.is_empty()).then_some(name);
    project.icon.clone_from(&self.icon);
    project.color.clone_from(&self.color);
    let avatar = avatar_look(&project, true, cx);
    h_flex()
      .gap_4()
      .items_center()
      .child(
        div()
          .flex()
          .items_center()
          .justify_center()
          .size_16()
          .flex_none()
          .rounded_lg()
          .text_xl()
          .font_semibold()
          .text_color(avatar.text)
          .when_some(avatar.fill, |badge, fill| badge.bg(fill))
          .map(|badge| match avatar.icon {
            Some(path) => badge.child(Icon::empty().path(path).size_8()),
            None => badge.child(avatar.letter),
          }),
      )
      .child(
        v_flex()
          .min_w_0()
          .gap_0p5()
          .child(div().truncate().text_xl().font_semibold().child(project.name()))
          .child(div().truncate().text_size(FONT_SIZE_SM).text_color(palette.text_secondary).child(display_path(&self.project.path))),
      )
  }

  /// The display name, and whether the folder on disk follows it.
  fn general(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let muted = cx.theme().muted_foreground;
    let name_field = v_flex().gap_2().child(crate::components::soft_input(&self.input, cx)).when(self.folder_differs(cx), |column| {
      column
        .child(Checkbox::new("rename-folder").label("Also rename the folder on disk").checked(self.rename_folder).on_click(cx.listener(
          |this, checked: &bool, _, cx| {
            this.rename_folder = *checked;
            cx.notify();
          },
        )))
        .child(
          Label::new("Terminals of this project restart and its open files close. Unsaved files block the rename.")
            .text_size(FONT_SIZE_SM)
            .text_color(palette.text_secondary),
        )
    });
    section("General", "The name shown in the project menu and on the project's tab.", field("Display name", muted, name_field), cx)
  }

  /// The color and the icon of the project's avatar.
  fn appearance(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let muted = cx.theme().muted_foreground;
    let default_color = self.color.is_none();
    let colors_row = h_flex()
      .gap_1()
      .items_center()
      .flex_wrap()
      .child(
        div()
          .id("color-none")
          .h_8()
          .px_3()
          .mr_1()
          .flex()
          .items_center()
          .rounded_full()
          .cursor_pointer()
          .text_size(FONT_SIZE_SM)
          .when(default_color, |pill| pill.bg(palette.accent).text_color(palette.on_accent))
          .when(!default_color, |pill| pill.bg(palette.surface_alt).text_color(palette.text_secondary).hover(|pill| pill.bg(palette.selected)))
          .child("Default")
          .on_click(cx.listener(|this, _, _, cx| {
            this.color = None;
            cx.notify();
          })),
      )
      .children((0..PROJECT_COLORS.len()).map(|index| self.swatch(index, cx)));
    let icons =
      h_flex().gap_0p5().flex_wrap().child(self.icon_button(None, cx)).children(PROJECT_ICONS.iter().map(|name| self.icon_button(Some(name), cx)));
    section(
      "Appearance",
      "The avatar of the project: a color, and an icon instead of its first letter.",
      v_flex().gap_4().child(field("Color", muted, colors_row)).child(field("Icon", muted, icons)),
      cx,
    )
  }

  fn git(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    section(
      "Git",
      "Update the remote address when the repository moves.",
      field("Git remote", muted, crate::components::soft_input(&self.remote_input, cx)),
      cx,
    )
  }

  /// Removing the project from Vekro; its folder stays on disk.
  fn danger(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let path = self.project.path.clone();
    section(
      "Remove project",
      "The project leaves the list and its terminals stop. The folder and its files stay on disk.",
      h_flex().child(
        Button::new("settings-remove")
          .label("Remove project")
          .icon(Icon::empty().path("icons/trash.svg"))
          .danger()
          .outline()
          .on_click(cx.listener(move |_, _, _, cx| cx.emit(ProjectSettingsEvent::RemoveRequested(path.clone())))),
      ),
      cx,
    )
    .border_color(palette.deleted.opacity(0.35))
  }
}

/// One block of the page: a title with a line of explanation, and its controls on a bordered surface.
fn section(title: &'static str, description: &'static str, content: impl IntoElement, cx: &App) -> Div {
  let palette = colors(cx);
  v_flex()
    .gap_4()
    .p_5()
    .rounded_lg()
    .border_1()
    .border_color(palette.border)
    .bg(palette.surface)
    .child(
      v_flex()
        .gap_0p5()
        .child(Label::new(title).font_weight(FontWeight::SEMIBOLD))
        .child(Label::new(description).text_size(FONT_SIZE_SM).text_color(palette.text_secondary)),
    )
    .child(content)
}

impl Render for ProjectSettings {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let dirty = self.is_dirty(cx);
    let page = v_flex()
      .w_full()
      .max_w(rems(44.))
      .mx_auto()
      .gap_5()
      .p_6()
      .child(self.header(cx))
      .child(self.general(cx))
      .child(self.appearance(cx))
      .child(self.git(cx))
      .child(self.danger(cx));
    v_flex()
      .size_full()
      .bg(palette.background)
      .text_size(FONT_SIZE)
      .child(
        div()
          .relative()
          .flex_1()
          .min_h_0()
          .child(div().id("project-settings-body").size_full().overflow_y_scroll().track_scroll(&self.scroll).child(page))
          .child(
            div().absolute().inset_0().child(
              Scrollbar::new(&self.scroll)
                .id("project-settings-scrollbar")
                .axis(ScrollbarAxis::Vertical)
                .mode(ScrollbarMode::Always)
                .viewport_from_layout(),
            ),
          ),
      )
      .child(
        h_flex()
          .justify_end()
          .gap_2()
          .px_6()
          .py_3()
          .border_t_1()
          .border_color(palette.border)
          .bg(palette.rail)
          .child(Button::new("settings-cancel").label(if dirty { "Discard" } else { "Close" }).on_click(cx.listener(|_, _, _, cx| {
            cx.emit(ProjectSettingsEvent::Close);
          })))
          .child(Button::new("settings-save").label("Save").primary().disabled(!dirty).on_click(cx.listener(|this, _, _, cx| this.save(cx)))),
      )
  }
}
