//! The Settings dialog.

use crate::components::selected_style;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    v_flex, ActiveTheme as _, Sizable as _,
  },
  *,
};
use std::path::PathBuf;
use vekro_storage::{data_dir, Preferences, ThemeChoice};

/// Sent with the new settings each time one changes.
pub(crate) struct PreferencesChanged(pub Preferences);

impl EventEmitter<PreferencesChanged> for Settings {}

pub(crate) struct Settings {
  preferences: Preferences,
  /// Where projects go when none is chosen: the folder shown while `preferences.projects_dir` is unset.
  fallback_dir: PathBuf,
}

impl Settings {
  pub fn new(preferences: Preferences, fallback_dir: PathBuf) -> Self {
    Self { preferences, fallback_dir }
  }

  fn set_theme(&mut self, theme: ThemeChoice, cx: &mut Context<Self>) {
    self.preferences.theme = theme;
    cx.emit(PreferencesChanged(self.preferences.clone()));
    cx.notify();
  }

  fn choose_projects_dir(&mut self, cx: &mut Context<Self>) {
    let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
    cx.spawn(async move |this, cx| {
      let Ok(Ok(Some(paths))) = picked.await else { return };
      let Some(path) = paths.into_iter().next() else { return };
      let _ = this.update(cx, |this, cx| {
        this.preferences.projects_dir = Some(path);
        cx.emit(PreferencesChanged(this.preferences.clone()));
        cx.notify();
      });
    })
    .detach();
  }

  fn reset_projects_dir(&mut self, cx: &mut Context<Self>) {
    self.preferences.projects_dir = None;
    cx.emit(PreferencesChanged(self.preferences.clone()));
    cx.notify();
  }

  fn theme_button(&self, id: &'static str, label: &'static str, choice: ThemeChoice, cx: &mut Context<Self>) -> impl IntoElement {
    let selected = self.preferences.theme == choice;
    Button::new(id)
      .label(label)
      .small()
      .when(selected, |button| button.custom(selected_style(cx)))
      .when(!selected, |button| button.outline())
      .on_click(cx.listener(move |this, _, _, cx| this.set_theme(choice, cx)))
  }
}

/// A titled group of settings.
fn section(title: &'static str, cx: &App) -> Div {
  v_flex().gap_2().child(Label::new(title).text_sm().font_weight(FontWeight::SEMIBOLD)).text_color(cx.theme().foreground)
}

impl Render for Settings {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let projects_dir = self.preferences.projects_dir.clone();
    let shown_dir = projects_dir.clone().unwrap_or_else(|| self.fallback_dir.clone());
    v_flex()
      .gap_5()
      .child(
        section("Appearance", cx)
          .child(
            h_flex()
              .gap_2()
              .child(self.theme_button("theme-system", "System", ThemeChoice::System, cx))
              .child(self.theme_button("theme-light", "Light", ThemeChoice::Light, cx))
              .child(self.theme_button("theme-dark", "Dark", ThemeChoice::Dark, cx)),
          )
          .child(Label::new("System follows the macOS appearance.").text_sm().text_color(muted)),
      )
      .child(
        section("New projects", cx).child(Label::new("New and cloned projects are created in this folder.").text_sm().text_color(muted)).child(
          h_flex()
            .gap_2()
            .items_center()
            .child(Label::new(shown_dir.display().to_string()).text_sm().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
            .child(
              Button::new("choose-projects-dir")
                .label("Change…")
                .small()
                .outline()
                .on_click(cx.listener(|this, _, _, cx| this.choose_projects_dir(cx))),
            )
            .when(projects_dir.is_some(), |row| {
              row.child(
                Button::new("reset-projects-dir").label("Reset").small().ghost().on_click(cx.listener(|this, _, _, cx| this.reset_projects_dir(cx))),
              )
            }),
        ),
      )
      .child(
        section("About", cx)
          .child(Label::new(format!("Vekro {}", env!("CARGO_PKG_VERSION"))).text_sm())
          .child(Label::new(format!("Data: {}", data_dir().display())).text_sm().text_color(muted)),
      )
  }
}
