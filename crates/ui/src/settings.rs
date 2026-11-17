//! The Settings dialog, built with the Kit's `Settings` component, and the model that holds the settings.

use crate::project_manager::ProjectManager;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    group_box::GroupBoxVariant,
    h_flex,
    label::Label,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    ActiveTheme as _, IconName, Sizable as _,
  },
  *,
};
use std::path::PathBuf;
use vekro_opencommit::{config_path, Config};
use vekro_storage::{data_dir, Preferences, ThemeChoice, CODE_FONT_SIZES};

/// The settings every window shares. Changing `preferences` and notifying makes each window save them and redraw.
pub(crate) struct SettingsModel {
  pub preferences: Preferences,
  /// The OpenCommit settings (`~/.opencommit`) that commit message generation uses.
  pub opencommit: Config,
}

/// Sent when the user asks to add every project in the projects folder.
pub(crate) struct AddAllProjectsRequested;

impl EventEmitter<AddAllProjectsRequested> for SettingsModel {}

/// Sent when the user asks to open the OpenCommit settings dialog.
pub(crate) struct CommitSettingsRequested;

impl EventEmitter<CommitSettingsRequested> for SettingsModel {}

/// The shared model, so the Kit's setting fields (which only get an `App`) can reach it.
pub(crate) struct SettingsGlobal(pub Entity<SettingsModel>);

impl Global for SettingsGlobal {}

/// Loads the saved settings into the shared model.
pub(crate) fn init_settings(cx: &mut App) -> Entity<SettingsModel> {
  let model = cx.new(|_| SettingsModel { preferences: Preferences::load(&Preferences::default_path()), opencommit: Config::load(&config_path()) });
  cx.set_global(SettingsGlobal(model.clone()));
  model
}

fn model(cx: &App) -> Entity<SettingsModel> {
  cx.global::<SettingsGlobal>().0.clone()
}

/// The size of code in the editor and in changes.
pub(crate) fn code_font_size(cx: &App) -> Pixels {
  px(preferences(cx).code_font_size as f32)
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
pub(crate) fn settings_view(fallback_dir: PathBuf, projects: Entity<ProjectManager>) -> Settings {
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
    ]),
    SettingPage::new("Editor").icon(IconName::File).resettable(false).group(
      SettingGroup::new().title("Code").item(
        SettingItem::new(
          "Font size",
          SettingField::number_input(
            NumberFieldOptions { min: *CODE_FONT_SIZES.start() as f64, max: *CODE_FONT_SIZES.end() as f64, step: 1.0 },
            |cx: &App| preferences(cx).code_font_size as f64,
            |value: f64, cx: &mut App| {
              let size = (value.round() as i64).clamp(*CODE_FONT_SIZES.start() as i64, *CODE_FONT_SIZES.end() as i64) as u32;
              update(cx, |preferences| preferences.code_font_size = size);
            },
          ),
        )
        .description("The size of code in the editor and in a file's changes, in pixels."),
      ),
    ),
    SettingPage::new("Projects").icon(IconName::Folder).resettable(false).groups(vec![
      SettingGroup::new()
        .title("Folder")
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
    SettingPage::new("Project bar").icon(IconName::Menu).resettable(false).group(
      SettingGroup::new()
        .title("Projects and categories")
        .description("Rename, recolor, reorder and group the projects of the bar on the left, or remove them. Drag a project onto another one in the bar to group them.")
        .item(SettingItem::render(move |_, _, _| projects.clone())),
    ),
    SettingPage::new("OpenCommit").icon(IconName::Bot).resettable(false).group(
      SettingGroup::new().title("Commit messages").item(
        SettingItem::new(
          "Generate commit messages",
          SettingField::render(|_, _, _| {
            Button::new("open-commit-settings")
              .label("Configure…")
              .small()
              .outline()
              .on_click(|_, _, cx| model(cx).update(cx, |_, cx| cx.emit(CommitSettingsRequested)))
          }),
        )
        .description("The sparkle button beside a commit's description writes the summary and description with OpenCommit. Choose the model, language and message style here."),
      ),
    ),
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
