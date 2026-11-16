//! The Settings dialog, built with the Kit's `Settings` component, and the model that holds the settings.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    group_box::GroupBoxVariant,
    h_flex,
    label::Label,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    ActiveTheme as _, IconName, Sizable as _,
  },
  *,
};
use std::path::PathBuf;
use vekro_storage::{data_dir, Preferences, ThemeChoice};

/// The settings every window shares. Changing `preferences` and notifying makes each window save them and redraw.
pub(crate) struct SettingsModel {
  pub preferences: Preferences,
}

/// Sent when the user asks to add every project in the projects folder.
pub(crate) struct AddAllProjectsRequested;

impl EventEmitter<AddAllProjectsRequested> for SettingsModel {}

/// The shared model, so the Kit's setting fields (which only get an `App`) can reach it.
pub(crate) struct SettingsGlobal(pub Entity<SettingsModel>);

impl Global for SettingsGlobal {}

/// Loads the saved settings into the shared model.
pub(crate) fn init_settings(cx: &mut App) -> Entity<SettingsModel> {
  let model = cx.new(|_| SettingsModel { preferences: Preferences::load(&Preferences::default_path()) });
  cx.set_global(SettingsGlobal(model.clone()));
  model
}

fn model(cx: &App) -> Entity<SettingsModel> {
  cx.global::<SettingsGlobal>().0.clone()
}

fn preferences(cx: &App) -> Preferences {
  model(cx).read(cx).preferences.clone()
}

/// Changes one setting and tells the windows.
fn update(cx: &mut App, change: impl FnOnce(&mut Preferences)) {
  model(cx).update(cx, |model, cx| {
    change(&mut model.preferences);
    cx.notify();
  });
}

/// Asks for a folder and makes it the projects folder.
fn choose_projects_folder(cx: &mut App) {
  let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
  cx.spawn(async move |cx| {
    let Ok(Ok(Some(paths))) = picked.await else { return };
    let Some(path) = paths.into_iter().next() else { return };
    cx.update(|cx| update(cx, |preferences| preferences.projects_dir = Some(path)));
  })
  .detach();
}

/// The Settings pages. `fallback_dir` is the projects folder shown while none is chosen.
pub(crate) fn settings_view(fallback_dir: PathBuf) -> Settings {
  let shown_dir = fallback_dir.clone();
  Settings::new("vekro-settings").with_group_variant(GroupBoxVariant::Outline).sidebar_width(px(180.)).pages(vec![
    SettingPage::new("General").icon(IconName::Settings).default_open(true).resettable(false).groups(vec![
      SettingGroup::new().title("Appearance").item(
        SettingItem::new(
          "Theme",
          SettingField::dropdown(
            vec![("system".into(), "System".into()), ("light".into(), "Light".into()), ("dark".into(), "Dark".into())],
            |cx: &App| {
              match preferences(cx).theme {
                ThemeChoice::System => "system",
                ThemeChoice::Light => "light",
                ThemeChoice::Dark => "dark",
              }
              .into()
            },
            |value: SharedString, cx: &mut App| {
              let theme = match value.as_ref() {
                "light" => ThemeChoice::Light,
                "dark" => ThemeChoice::Dark,
                _ => ThemeChoice::System,
              };
              update(cx, |preferences| preferences.theme = theme);
            },
          ),
        )
        .description("System follows the macOS appearance."),
      ),
      SettingGroup::new()
        .title("Projects")
        .item(
          SettingItem::new(
            "Projects folder",
            SettingField::render(move |_, _, cx| {
              let chosen = preferences(cx).projects_dir;
              let path = chosen.clone().unwrap_or_else(|| shown_dir.clone());
              h_flex()
                .gap_2()
                .items_center()
                .child(
                  Label::new(crate::components::display_path(&path))
                    .text_color(cx.theme().muted_foreground)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis(),
                )
                .child(Button::new("choose-projects-folder").label("Change…").small().outline().on_click(|_, _, cx| choose_projects_folder(cx)))
                .when(chosen.is_some(), |row| {
                  row.child(
                    Button::new("reset-projects-folder")
                      .label("Reset")
                      .small()
                      .ghost()
                      .on_click(|_, _, cx| update(cx, |preferences| preferences.projects_dir = None)),
                  )
                })
            }),
          )
          .description("New and cloned projects are created here, and the Add project dialog starts here.")
          .layout(Axis::Vertical),
        )
        .item(
          SettingItem::new(
            "Add every project in the folder",
            SettingField::render(|_, _, _| {
              Button::new("add-all-projects")
                .label("Add all")
                .small()
                .outline()
                .on_click(|_, _, cx| model(cx).update(cx, |_, cx| cx.emit(AddAllProjectsRequested)))
            }),
          )
          .description("Adds every Git repository directly inside the projects folder to the project bar."),
        ),
    ]),
    SettingPage::new("About").icon(IconName::Info).resettable(false).group(
      SettingGroup::new()
        .title("Vekro")
        .item(SettingItem::new("Version", SettingField::render(|_, _, _| Label::new(env!("CARGO_PKG_VERSION")).text_sm())))
        .item(SettingItem::new(
          "Data folder",
          SettingField::render(|_, _, cx| Label::new(crate::components::display_path(&data_dir())).text_sm().text_color(cx.theme().muted_foreground)),
        )),
    ),
  ])
}
