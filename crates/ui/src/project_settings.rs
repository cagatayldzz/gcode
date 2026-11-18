//! The dialog that sets how a project looks in the project menu (its name, icon and color) and where its Git remote points.
//! Like Settings, it has a page list on the left (Kit `Settings`) and a Save / Cancel footer.

use crate::{
  components::selected_style,
  theme::{colors, project_color, FONT_SIZE, FONT_SIZE_SM, PROJECT_COLORS},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings},
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
  /// The new remote address, only when the user changed it.
  pub remote: Option<String>,
}

/// One control of the dialog, shown inside a page of the Kit `Settings`.
#[derive(Clone, Copy)]
enum Part {
  Name,
  Color,
  Icon,
  Remote,
}

pub(crate) struct ProjectSettings {
  path: PathBuf,
  title: SharedString,
  input: Entity<InputState>,
  remote_input: Entity<InputState>,
  remote: String,
  icon: Option<String>,
  color: Option<String>,
  name_part: Entity<SettingsPart>,
  color_part: Entity<SettingsPart>,
  icon_part: Entity<SettingsPart>,
  remote_part: Entity<SettingsPart>,
  _subscription: Subscription,
}

impl EventEmitter<AppearanceChosen> for ProjectSettings {}

impl ProjectSettings {
  pub fn new(project: &Project, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let folder = project.folder_name();
    let current = project.name();
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(folder).default_value(current));
    let remote = vekro_git::remote_url(&project.path).unwrap_or_default();
    let remote_input = cx.new(|cx| InputState::new(window, cx).placeholder("https://github.com/owner/repository.git").default_value(remote.clone()));
    let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
      if let InputEvent::PressEnter { .. } = event {
        this.save(cx);
      }
    });
    let this = cx.entity();
    let part = |part: Part, cx: &mut Context<Self>| {
      let settings = this.clone();
      cx.new(|_| SettingsPart { settings, part })
    };
    Self {
      path: project.path.clone(),
      title: SharedString::from(format!("{} settings", project.name())),
      input,
      remote_input,
      remote,
      icon: project.icon.clone(),
      color: project.color.clone(),
      name_part: part(Part::Name, cx),
      color_part: part(Part::Color, cx),
      icon_part: part(Part::Icon, cx),
      remote_part: part(Part::Remote, cx),
      _subscription: subscription,
    }
  }

  fn save(&mut self, cx: &mut Context<Self>) {
    let name = self.input.read(cx).value().to_string();
    let remote = self.remote_input.read(cx).value().trim().to_string();
    let remote = (remote != self.remote && !remote.is_empty()).then_some(remote);
    cx.emit(AppearanceChosen { path: self.path.clone(), name, icon: self.icon.clone(), color: self.color.clone(), remote });
  }
}

/// A control of [`ProjectSettings`] as its own view, so that a page of the Kit `Settings` can hold it.
struct SettingsPart {
  settings: Entity<ProjectSettings>,
  part: Part,
}

impl SettingsPart {
  fn swatch(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
    let (name, value) = PROJECT_COLORS[index];
    let chosen = self.settings.read(cx).color.as_deref() == Some(value);
    let theme = cx.theme().clone();
    let settings = self.settings.clone();
    div()
      .id(SharedString::from(format!("color-{name}")))
      .size_6()
      .rounded_full()
      .cursor_pointer()
      .border_2()
      .border_color(if chosen { theme.foreground } else { theme.transparent })
      .when_some(project_color(value), |swatch, color| swatch.bg(color))
      .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name).build(window, cx))
      .on_click(move |_, _, cx| choose_color(&settings, Some(value), cx))
  }

  fn icon_button(&self, name: Option<&'static str>, cx: &mut Context<Self>) -> impl IntoElement {
    let chosen = self.settings.read(cx).icon.as_deref() == name;
    let id = SharedString::from(format!("icon-{}", name.unwrap_or("letter")));
    let settings = self.settings.clone();
    Button::new(id)
      .small()
      .when(chosen, |button| button.custom(selected_style(cx)))
      .when(!chosen, |button| button.ghost())
      .when_some(name, |button, name| button.icon(Icon::empty().path(format!("icons/{name}.svg"))))
      .when(name.is_none(), |button| button.label("Aa"))
      .on_click(move |_, _, cx| {
        settings.update(cx, |this, cx| {
          this.icon = name.map(str::to_owned);
          cx.notify();
        })
      })
  }
}

fn choose_color(settings: &Entity<ProjectSettings>, color: Option<&'static str>, cx: &mut App) {
  settings.update(cx, |this, cx| {
    this.color = color.map(str::to_owned);
    cx.notify();
  });
}

impl Render for SettingsPart {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let state = self.settings.read(cx);
    match self.part {
      Part::Name => div().w_full().text_size(FONT_SIZE).child(Input::new(&state.input)).into_any_element(),
      Part::Remote => v_flex()
        .w_full()
        .gap_1p5()
        .text_size(FONT_SIZE)
        .child(Input::new(&state.remote_input))
        .child(Label::new("Change it when the repository was renamed or moved.").text_color(muted).text_size(FONT_SIZE_SM))
        .into_any_element(),
      Part::Color => {
        let settings = self.settings.clone();
        let default_color = Button::new("color-none")
          .small()
          .label("Default")
          .when(state.color.is_none(), |button| button.custom(selected_style(cx)))
          .when(state.color.is_some(), |button| button.ghost())
          .on_click(move |_, _, cx| choose_color(&settings, None, cx));
        h_flex()
          .gap_2()
          .items_center()
          .flex_wrap()
          .text_size(FONT_SIZE)
          .child(default_color)
          .children((0..PROJECT_COLORS.len()).map(|index| self.swatch(index, cx)))
          .into_any_element()
      }
      Part::Icon => h_flex()
        .gap_1()
        .flex_wrap()
        .child(self.icon_button(None, cx))
        .children(PROJECT_ICONS.iter().map(|name| self.icon_button(Some(name), cx)))
        .into_any_element(),
    }
  }
}

impl Render for ProjectSettings {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let (name, color, icon, remote) = (self.name_part.clone(), self.color_part.clone(), self.icon_part.clone(), self.remote_part.clone());
    let pages =
      Settings::new("vekro-project-settings").sidebar_width(px(180.)).sidebar_style(&StyleRefinement::default().bg(palette.background)).pages(vec![
        SettingPage::new("Appearance").default_open(true).resettable(false).group(
          SettingGroup::new()
            .title("Look in the project menu")
            .item(SettingItem::new("Name", SettingField::render(move |_, _, _| name.clone())).layout(Axis::Vertical))
            .item(SettingItem::new("Color", SettingField::render(move |_, _, _| color.clone())).layout(Axis::Vertical))
            .item(SettingItem::new("Icon", SettingField::render(move |_, _, _| icon.clone())).layout(Axis::Vertical)),
        ),
        SettingPage::new("Git").resettable(false).group(
          SettingGroup::new()
            .title("Remote")
            .item(SettingItem::new("Git remote", SettingField::render(move |_, _, _| remote.clone())).layout(Axis::Vertical)),
        ),
      ]);
    // The dialog's own padding is removed so that the page list reaches the edges of the window.
    v_flex()
      .rounded_b_lg()
      .overflow_hidden()
      .child(
        div()
          .border_b_1()
          .border_color(palette.border)
          .child(Label::new(self.title.clone()).text_size(FONT_SIZE).font_weight(FontWeight::SEMIBOLD).px_5().py_4()),
      )
      .child(div().h(px(400.)).child(pages))
      .child(
        h_flex()
          .gap_2()
          .justify_end()
          .px_5()
          .py_3()
          .border_t_1()
          .border_color(palette.border)
          .child(Button::new("settings-cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
          .child(Button::new("settings-save").label("Save").primary().on_click(cx.listener(|this, _, _, cx| this.save(cx)))),
      )
  }
}
