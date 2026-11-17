//! The dialog that sets how a project looks in the project menu: its name, icon and color.

use crate::{
  components::selected_style,
  theme::{project_color, FONT_SIZE, PROJECT_COLORS},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    v_flex, ActiveTheme, Icon, Sizable as _, WindowExt as _,
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
}

pub(crate) struct ProjectSettings {
  path: PathBuf,
  input: Entity<InputState>,
  icon: Option<String>,
  color: Option<String>,
  _subscription: Subscription,
}

impl EventEmitter<AppearanceChosen> for ProjectSettings {}

impl ProjectSettings {
  pub fn new(project: &Project, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let folder = project.folder_name();
    let current = project.name();
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(folder).default_value(current));
    let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
      if let InputEvent::PressEnter { .. } = event {
        this.save(cx);
      }
    });
    Self { path: project.path.clone(), input, icon: project.icon.clone(), color: project.color.clone(), _subscription: subscription }
  }

  fn save(&mut self, cx: &mut Context<Self>) {
    let name = self.input.read(cx).value().to_string();
    cx.emit(AppearanceChosen { path: self.path.clone(), name, icon: self.icon.clone(), color: self.color.clone() });
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
        this.color = Some(value.to_string());
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
      .on_click(cx.listener(move |this, _, _, cx| {
        this.icon = name.map(str::to_owned);
        cx.notify();
      }))
  }
}

impl Render for ProjectSettings {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let muted = theme.muted_foreground;
    let default_color = Button::new("color-none")
      .small()
      .label("Default")
      .when(self.color.is_none(), |button| button.custom(selected_style(cx)))
      .when(self.color.is_some(), |button| button.ghost());
    v_flex()
      .text_size(FONT_SIZE)
      .gap_4()
      .child(v_flex().gap_2().child(Label::new("Name").text_color(muted)).child(Input::new(&self.input)))
      .child(
        v_flex().gap_2().child(Label::new("Color").text_color(muted)).child(
          h_flex()
            .gap_2()
            .items_center()
            .flex_wrap()
            .child(default_color.on_click(cx.listener(|this, _, _, cx| {
              this.color = None;
              cx.notify();
            })))
            .children((0..PROJECT_COLORS.len()).map(|index| self.swatch(index, cx))),
        ),
      )
      .child(v_flex().gap_2().child(Label::new("Icon").text_color(muted)).child(
        h_flex().gap_1().flex_wrap().child(self.icon_button(None, cx)).children(PROJECT_ICONS.iter().map(|name| self.icon_button(Some(name), cx))),
      ))
      .child(
        h_flex()
          .gap_2()
          .justify_end()
          .child(Button::new("settings-cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
          .child(Button::new("settings-save").label("Save").primary().on_click(cx.listener(|this, _, _, cx| this.save(cx)))),
      )
  }
}
