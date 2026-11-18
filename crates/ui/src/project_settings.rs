//! The dialog that sets how a project looks in the project menu (its name, icon and color) and where its Git remote points.
//! Appearance and Git settings share a footer with project management controls.

use crate::{
  components::{avatar_look, display_path, selected_style},
  theme::{colors, project_color, PROJECT_COLORS},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    tab::{Tab, TabBar},
    v_flex, ActiveTheme, Icon, Sizable as _, StyledExt as _, WindowExt as _,
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
}

pub(crate) enum ProjectSettingsEvent {
  Save(AppearanceChosen),
  RemoveRequested(PathBuf),
}

pub(crate) struct ProjectSettings {
  project: Project,
  title: SharedString,
  input: Entity<InputState>,
  remote_input: Entity<InputState>,
  remote: String,
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
      title: SharedString::from(project.name()),
      input,
      remote_input,
      remote,
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
    }));
  }

  fn swatch(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
    let (name, value) = PROJECT_COLORS[index];
    let chosen = self.color.as_deref() == Some(value);
    let theme = cx.theme().clone();
    div()
      .id(SharedString::from(format!("color-{name}")))
      .size_6()
      .rounded_full()
      .cursor_pointer()
      .border_2()
      .border_color(if chosen { theme.foreground } else { theme.transparent })
      .when_some(project_color(value), |swatch, color| swatch.bg(color))
      .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name).build(window, cx))
      .on_click(cx.listener(move |this, _, _, cx| {
        this.color = Some(value.into());
        cx.notify();
      }))
  }

  fn icon_button(&self, name: Option<&'static str>, cx: &mut Context<Self>) -> impl IntoElement {
    let chosen = self.icon.as_deref() == name;
    let id = SharedString::from(format!("icon-{}", name.unwrap_or("letter")));
    Button::new(id)
      .small()
      .when(chosen, |button| button.custom(selected_style(cx)))
      .when(!chosen, |button| button.ghost())
      .when_some(name, |button, name| button.icon(Icon::empty().path(format!("icons/{name}.svg"))))
      .when(name.is_none(), |button| button.label("Aa"))
      .accessibility_label(name.unwrap_or("Use project initial"))
      .on_click(cx.listener(move |this, _, _, cx| {
        this.icon = name.map(str::to_owned);
        cx.notify();
      }))
  }

  fn appearance(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let mut project = self.project.clone();
    let name = self.input.read(cx).value().trim().to_string();
    project.name = (!name.is_empty()).then_some(name);
    project.icon.clone_from(&self.icon);
    project.color.clone_from(&self.color);
    let avatar = avatar_look(&project, true, cx);
    let preview = h_flex()
      .gap_3()
      .rounded_md()
      .border_1()
      .border_color(palette.border)
      .bg(palette.surface_alt)
      .p_3()
      .child(
        div()
          .flex()
          .items_center()
          .justify_center()
          .size_10()
          .flex_none()
          .rounded_md()
          .font_semibold()
          .text_color(avatar.text)
          .when_some(avatar.fill, |badge, fill| badge.bg(fill))
          .map(|badge| match avatar.icon {
            Some(path) => badge.child(Icon::empty().path(path).size_5()),
            None => badge.child(avatar.letter),
          }),
      )
      .child(
        v_flex()
          .min_w_0()
          .gap_1()
          .child(Label::new("Project menu preview").text_xs().text_color(palette.text_secondary))
          .child(div().truncate().text_sm().font_semibold().child(project.name())),
      );
    v_flex()
      .gap_4()
      .child(preview)
      .child(v_flex().gap_2().child(Label::new("Display name").font_semibold()).child(Input::new(&self.input)))
      .child(
        v_flex().gap_2().child(Label::new("Color").font_semibold()).child(
          h_flex()
            .gap_2()
            .flex_wrap()
            .child(
              Button::new("color-none").small().label("Default").when(self.color.is_none(), |button| button.custom(selected_style(cx))).on_click(
                cx.listener(|this, _, _, cx| {
                  this.color = None;
                  cx.notify();
                }),
              ),
            )
            .children((0..PROJECT_COLORS.len()).map(|index| self.swatch(index, cx))),
        ),
      )
      .child(v_flex().gap_2().child(Label::new("Icon").font_semibold()).child(
        h_flex().gap_1().flex_wrap().child(self.icon_button(None, cx)).children(PROJECT_ICONS.iter().map(|name| self.icon_button(Some(name), cx))),
      ))
  }
}

impl Render for ProjectSettings {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let tabs = TabBar::new("project-settings-tabs")
      .selected_index(self.tab)
      .on_click(cx.listener(|this, index: &usize, _, cx| {
        this.tab = *index;
        cx.notify();
      }))
      .child(Tab::new().cursor_pointer().label("Appearance"))
      .child(Tab::new().cursor_pointer().label("Git"));
    let body = if self.tab == 0 {
      self.appearance(cx).into_any_element()
    } else {
      v_flex()
        .gap_2()
        .child(Label::new("Git remote").font_semibold())
        .child(Input::new(&self.remote_input))
        .child(Label::new("Update the remote URL when the repository moves.").text_xs().text_color(palette.text_secondary))
        .into_any_element()
    };
    v_flex()
      .overflow_hidden()
      .child(
        v_flex()
          .gap_1()
          .px_6()
          .pt_5()
          .pb_3()
          .child(Label::new(self.title.clone()).text_lg().font_semibold())
          .child(div().truncate().text_xs().text_color(palette.text_secondary).child(display_path(&self.project.path))),
      )
      .child(div().px_6().border_b_1().border_color(palette.border).child(tabs))
      .child(div().id("project-settings-body").h(rems(23.)).overflow_y_scroll().px_6().py_4().child(body))
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
              .outline()
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
