//! The dialog that sets how a project looks in the project menu (its name, icon and color) and where its Git remote points.
//! Appearance and Git settings share a footer with project management controls.

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
    tab::{Tab, TabBar},
    v_flex, ActiveTheme, Icon, StyledExt as _, WindowExt as _,
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
}

pub(crate) struct ProjectSettings {
  project: Project,
  input: Entity<InputState>,
  remote_input: Entity<InputState>,
  remote: String,
  rename_folder: bool,
  icon: Option<String>,
  color: Option<String>,
  tab: usize,
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
      tab: 0,
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
      .gap_3()
      .items_center()
      .px_6()
      .pt_5()
      .pb_4()
      .child(
        div()
          .flex()
          .items_center()
          .justify_center()
          .size_12()
          .flex_none()
          .rounded_lg()
          .text_lg()
          .font_semibold()
          .text_color(avatar.text)
          .when_some(avatar.fill, |badge, fill| badge.bg(fill))
          .map(|badge| match avatar.icon {
            Some(path) => badge.child(Icon::empty().path(path).size_6()),
            None => badge.child(avatar.letter),
          }),
      )
      .child(
        v_flex()
          .min_w_0()
          .gap_0p5()
          .child(div().truncate().text_lg().font_semibold().child(project.name()))
          .child(div().truncate().text_size(FONT_SIZE_SM).text_color(palette.text_secondary).child(display_path(&self.project.path))),
      )
  }

  fn appearance(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let muted = cx.theme().muted_foreground;
    let default_color = self.color.is_none();
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
    let icons = h_flex()
      .gap_0p5()
      .flex_wrap()
      .p_1()
      .rounded_md()
      .border_1()
      .border_color(palette.border)
      .child(self.icon_button(None, cx))
      .children(PROJECT_ICONS.iter().map(|name| self.icon_button(Some(name), cx)));
    v_flex().gap_4().child(field("Display name", muted, name_field)).child(field("Color", muted, colors_row)).child(field("Icon", muted, icons))
  }
}

impl Render for ProjectSettings {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let muted = cx.theme().muted_foreground;
    let tab = |label: &'static str, icon: &'static str| {
      Tab::new().cursor_pointer().child(h_flex().gap_2().items_center().child(Icon::empty().path(icon).size_4()).child(label))
    };
    let tabs = TabBar::new("project-settings-tabs")
      .segmented()
      .selected_index(self.tab)
      .on_click(cx.listener(|this, index: &usize, _, cx| {
        this.tab = *index;
        cx.notify();
      }))
      .child(tab("Appearance", "icons/palette.svg"))
      .child(tab("Git", "icons/git-branch.svg"));
    let body = if self.tab == 0 {
      self.appearance(cx).into_any_element()
    } else {
      v_flex()
        .gap_2()
        .child(field("Git remote", muted, crate::components::soft_input(&self.remote_input, cx)))
        .child(Label::new("Update the remote URL when the repository moves.").text_size(FONT_SIZE_SM).text_color(palette.text_secondary))
        .into_any_element()
    };
    v_flex()
      .overflow_hidden()
      .text_size(FONT_SIZE)
      .child(self.header(cx))
      .child(div().px_6().pb_4().child(tabs))
      .child(div().id("project-settings-body").h(rems(21.)).overflow_y_scroll().px_6().pb_5().child(body))
      .child(
        h_flex()
          .justify_between()
          .gap_3()
          .px_6()
          .py_3()
          .border_t_1()
          .border_color(palette.border)
          .child(
            Button::new("settings-remove")
              .label("Remove project")
              .icon(Icon::empty().path("icons/trash.svg"))
              .danger()
              .ghost()
              .on_click(cx.listener(|this, _, _, cx| cx.emit(ProjectSettingsEvent::RemoveRequested(this.project.path.clone())))),
          )
          .child(
            h_flex()
              .gap_2()
              .child(Button::new("settings-cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
              .child(Button::new("settings-save").label("Save").primary().on_click(cx.listener(|this, _, _, cx| this.save(cx)))),
          ),
      )
  }
}
