//! The Settings dialog, built with the Kit's `Settings` component, and the model that holds the settings.

use crate::{
  connections::AccountsList,
  profiles::ProfileManager,
  project_manager::ProjectManager,
  updates::{UpdatePanel, UpdateState, UPDATES_ENABLED},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    group_box::GroupBoxVariant,
    h_flex,
    label::Label,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
  },
  *,
};
use std::collections::HashMap;
use std::path::PathBuf;
use vekro_hosting::Accounts;
use vekro_opencommit::{config_path, Config};
use vekro_storage::{data_dir, Preferences, Profile, ThemeChoice, FONT_SIZES};

/// The settings of one profile, shared by every window that shows it. Changing `preferences` and notifying makes each of those
/// windows save them and redraw.
pub(crate) struct SettingsModel {
  /// The profile whose files these settings are read from and saved to.
  pub profile: Profile,
  pub preferences: Preferences,
  /// The OpenCommit settings (`~/.opencommit`) that commit message generation uses.
  pub opencommit: Config,
  /// The connected hosting accounts (`accounts.json`); their tokens are in the Keychain.
  pub accounts: Accounts,
  /// Vekro's own updates.
  pub updates: UpdateState,
  /// The accounts whose permissions are being checked, by key.
  pub rechecking: Vec<String>,
}

/// Sent when the user asks to add every project in the projects folder.
pub(crate) struct AddAllProjectsRequested;

impl EventEmitter<AddAllProjectsRequested> for SettingsModel {}

/// Sent when the user asks to connect a hosting account.
pub(crate) struct ConnectAccountRequested;

impl EventEmitter<ConnectAccountRequested> for SettingsModel {}

/// Sent when the user asks to check again the permissions of the account with this key.
pub(crate) struct AccountRecheckRequested(pub String);

impl EventEmitter<AccountRecheckRequested> for SettingsModel {}

/// Sent when the user disconnects the account with this key.
pub(crate) struct AccountRemoveRequested(pub String);

impl EventEmitter<AccountRemoveRequested> for SettingsModel {}

/// Sent when the user asks to open the OpenCommit settings dialog.
pub(crate) struct CommitSettingsRequested;

impl EventEmitter<CommitSettingsRequested> for SettingsModel {}

/// The buttons of the activity bar, by id and name, in their default order.
pub(crate) const ACTIVITY_ITEMS: [(&str, &str); 5] =
  [("explorer", "Explorer"), ("search", "Search"), ("git", "Git"), ("terminal", "Terminal"), ("agents", "AI agents")];

/// The ids of [`ACTIVITY_ITEMS`].
pub(crate) fn activity_ids() -> Vec<&'static str> {
  ACTIVITY_ITEMS.iter().map(|(id, _)| *id).collect()
}

/// The activity bar's buttons in the order the user chose, with whether each is shown.
pub(crate) fn activity_layout(model: &Entity<SettingsModel>, cx: &App) -> Vec<(&'static str, bool)> {
  let layout = model.read(cx).preferences.activity_bar.clone();
  layout.arranged(&activity_ids()).into_iter().map(|id| (id, !layout.is_hidden(id))).collect()
}

/// One row of the activity bar page: the name, the arrows that move the button and the switch that shows or hides it.
fn activity_row(store: &Store, index: usize, count: usize, id: &'static str, shown: bool, cx: &App) -> impl IntoElement {
  let name = ACTIVITY_ITEMS.iter().find(|(known, _)| *known == id).map_or(id, |(_, name)| *name);
  let muted = cx.theme().muted_foreground;
  h_flex()
    .gap_2()
    .items_center()
    .py_1()
    .child(Label::new(name).flex_1().when(!shown, |label| label.text_color(muted)))
    .child(
      Button::new(SharedString::from(format!("activity-up-{id}")))
        .ghost()
        .xsmall()
        .icon(Icon::new(IconName::ChevronUp))
        .tooltip("Move up")
        .disabled(index == 0)
        .on_click({
          let store = store.clone();
          move |_, _, cx| store.update(cx, |preferences| preferences.activity_bar.move_by(&activity_ids(), id, -1))
        }),
    )
    .child(
      Button::new(SharedString::from(format!("activity-down-{id}")))
        .ghost()
        .xsmall()
        .icon(Icon::new(IconName::ChevronDown))
        .tooltip("Move down")
        .disabled(index + 1 == count)
        .on_click({
          let store = store.clone();
          move |_, _, cx| store.update(cx, |preferences| preferences.activity_bar.move_by(&activity_ids(), id, 1))
        }),
    )
    .child(Switch::new(SharedString::from(format!("activity-show-{id}"))).checked(shown).on_click({
      let store = store.clone();
      move |checked, _, cx| store.update(cx, |preferences| preferences.activity_bar.set_hidden(id, !*checked))
    }))
}

/// The settings models by profile id: windows of one profile share a model, windows of different profiles do not.
#[derive(Default)]
pub(crate) struct SettingsRegistry {
  models: HashMap<String, Entity<SettingsModel>>,
}

impl Global for SettingsRegistry {}

/// The model of a profile, loaded from its files the first time a window shows the profile.
pub(crate) fn settings_for(profile: &Profile, cx: &mut App) -> Entity<SettingsModel> {
  if let Some(model) = cx.try_global::<SettingsRegistry>().and_then(|registry| registry.models.get(&profile.id)) {
    return model.clone();
  }
  let first = !cx.has_global::<SettingsRegistry>();
  let model = cx.new(|_| SettingsModel {
    profile: profile.clone(),
    preferences: Preferences::load(&profile.preferences_path()),
    opencommit: Config::load(&config_path()),
    accounts: Accounts::load_in(&profile.accounts_path(), profile.keychain_namespace()),
    updates: UpdateState::new(),
    rechecking: Vec::new(),
  });
  if first {
    cx.set_global(SettingsRegistry::default());
  }
  cx.global_mut::<SettingsRegistry>().models.insert(profile.id.clone(), model.clone());
  // The first window asks GitHub for a newer release, unless that is turned off.
  if first && UPDATES_ENABLED && model.read(cx).preferences.check_updates {
    model.update(cx, |model, cx| model.check_update(cx));
  }
  model
}

/// Forgets a profile's model, once no window shows the profile any more, so that it is read from disk again next time.
pub(crate) fn release_settings(profile_id: &str, cx: &mut App) {
  if cx.has_global::<SettingsRegistry>() {
    cx.global_mut::<SettingsRegistry>().models.remove(profile_id);
  }
}

/// One profile's settings, for the Kit's setting fields (which only get an `App`).
#[derive(Clone)]
pub(crate) struct Store(pub Entity<SettingsModel>);

impl Store {
  fn preferences(&self, cx: &App) -> Preferences {
    self.0.read(cx).preferences.clone()
  }

  /// Changes one setting and tells the windows.
  fn update(&self, cx: &mut App, change: impl FnOnce(&mut Preferences)) {
    self.0.update(cx, |model, cx| {
      change(&mut model.preferences);
      cx.notify();
    });
  }

  fn emit<E: 'static>(&self, event: E, cx: &mut App)
  where
    SettingsModel: EventEmitter<E>,
  {
    self.0.update(cx, |_, cx| cx.emit(event));
  }
}

/// The size of code in the editor and in changes.
pub(crate) fn code_font_size(model: &Entity<SettingsModel>, cx: &App) -> Pixels {
  px(model.read(cx).preferences.code_font_size as f32)
}

/// Asks for a folder and makes it the projects folder.
fn choose_projects_folder(store: Store, cx: &mut App) {
  let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
  cx.spawn(async move |cx| {
    let Ok(Ok(Some(paths))) = picked.await else { return };
    let Some(path) = paths.into_iter().next() else { return };
    cx.update(|cx| store.update(cx, |preferences| preferences.projects_dir = Some(path)));
  })
  .detach();
}

fn font_size_item(store: &Store, title: &'static str, description: &'static str, size: fn(&mut Preferences) -> &mut u32) -> SettingItem {
  let min = *FONT_SIZES.start() as f64;
  let max = *FONT_SIZES.end() as f64;
  let (read, write) = (store.clone(), store.clone());
  SettingItem::new(
    title,
    SettingField::number_input(
      NumberFieldOptions { min, max, step: 1.0 },
      move |cx: &App| *size(&mut read.preferences(cx)) as f64,
      move |value: f64, cx: &mut App| write.update(cx, |preferences| *size(preferences) = value.round().clamp(min, max) as u32),
    ),
  )
  .description(description)
}

/// The Settings pages. `fallback_dir` is the projects folder shown while none is chosen.
pub(crate) fn settings_view(
  model: Entity<SettingsModel>,
  fallback_dir: PathBuf,
  projects: Entity<ProjectManager>,
  profiles: Entity<ProfileManager>,
  accounts: Entity<AccountsList>,
  updates: Entity<UpdatePanel>,
  first_page: &str,
  background: Hsla,
) -> Settings {
  let shown_dir = fallback_dir.clone();
  let store = Store(model);
  let (theme_get, theme_set, layout_store, folder_store, add_all_store, commit_store, updates_get, updates_set) =
    (store.clone(), store.clone(), store.clone(), store.clone(), store.clone(), store.clone(), store.clone(), store.clone());
  Settings::new("vekro-settings").with_group_variant(GroupBoxVariant::Outline).sidebar_width(px(180.)).sidebar_style(&StyleRefinement::default().bg(background)).pages(vec![
    SettingPage::new("General").default_open(first_page == "General").resettable(false).groups(vec![
      SettingGroup::new().title("Appearance").item(
        SettingItem::new(
          "Theme",
          SettingField::dropdown(
            vec![("system".into(), "System".into()), ("light".into(), "Light".into()), ("dark".into(), "Dark".into())],
            move |cx: &App| {
              match theme_get.preferences(cx).theme {
                ThemeChoice::System => "system",
                ThemeChoice::Light => "light",
                ThemeChoice::Dark => "dark",
              }
              .into()
            },
            move |value: SharedString, cx: &mut App| {
              let theme = match value.as_ref() {
                "light" => ThemeChoice::Light,
                "dark" => ThemeChoice::Dark,
                _ => ThemeChoice::System,
              };
              theme_set.update(cx, |preferences| preferences.theme = theme);
            },
          ),
        )
        .description("System follows the macOS appearance."),
      ),
    ]),
    SettingPage::new("Editor")
      .default_open(first_page == "Editor")
      .resettable(false)
      .group(
        SettingGroup::new().title("Code").item(font_size_item(
          &store,
          "Font size",
          "The size of code in the editor and in a file's changes, in pixels.",
          |preferences| &mut preferences.code_font_size,
        )),
      )
      .group(
        SettingGroup::new().title("Explorer").item(font_size_item(
          &store,
          "Explorer font size",
          "The size of file and folder names in Explorer, in pixels.",
          |preferences| &mut preferences.explorer_font_size,
        )),
      ),
    SettingPage::new("Activity bar").default_open(first_page == "Activity bar").resettable(false).group(
      SettingGroup::new()
        .title("Buttons")
        .description("Order the buttons of the bar at the left edge and choose which ones show. A hidden button's shortcut still works.")
        .item(SettingItem::render(move |_, _, cx| {
          let layout = activity_layout(&layout_store.0, cx);
          let store = layout_store.clone();
          let count = layout.len();
          v_flex()
            .w_full()
            .children(layout.into_iter().enumerate().map(|(index, (id, shown))| activity_row(&store, index, count, id, shown, cx)))
            .child(
              h_flex().pt_2().child(
                Button::new("activity-reset")
                  .label("Reset to default")
                  .small()
                  .outline()
                  .on_click({
                    let store = store.clone();
                    move |_, _, cx| store.update(cx, |preferences| preferences.activity_bar = Default::default())
                  }),
              ),
            )
        })),
    ),
    SettingPage::new("Projects").default_open(first_page == "Projects").resettable(false).groups(vec![
      SettingGroup::new()
        .title("Folder")
        .item(
          SettingItem::new(
            "Projects folder",
            SettingField::render(move |_, _, cx| {
              let chosen = folder_store.preferences(cx).projects_dir;
              let store = folder_store.clone();
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
                .child({
                  let store = store.clone();
                  Button::new("choose-projects-folder").label("Change…").small().outline().on_click(move |_, _, cx| choose_projects_folder(store.clone(), cx))
                })
                .when(chosen.is_some(), |row| {
                  row.child(
                    Button::new("reset-projects-folder")
                      .label("Reset")
                      .small()
                      .ghost()
                      .on_click(move |_, _, cx| store.update(cx, |preferences| preferences.projects_dir = None)),
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
            SettingField::render(move |_, _, _| {
              let store = add_all_store.clone();
              Button::new("add-all-projects").label("Add all").small().outline().on_click(move |_, _, cx| store.emit(AddAllProjectsRequested, cx))
            }),
          )
          .description("Adds every Git repository directly inside the projects folder to the project menu."),
        ),
    ]),
    SettingPage::new("Project list").default_open(first_page == "Project list").resettable(false).group(
      SettingGroup::new()
        .title("Projects and categories")
        .description("Rename, recolor, reorder and group the projects of the menu in the title bar, or remove them. Right-click a project in that menu to move it to a category.")
        .item(SettingItem::render(move |_, _, _| projects.clone())),
    ),
    SettingPage::new("Profiles").default_open(first_page == "Profiles").resettable(false).group(
      SettingGroup::new()
        .title("Profiles")
        .description("Each profile keeps its own projects, settings and connected accounts, like a browser profile. Choose one from the Profiles menu to open its window.")
        .item(SettingItem::render(move |_, _, _| profiles.clone())),
    ),
    SettingPage::new("Accounts").default_open(first_page == "Accounts").resettable(false).group(
      SettingGroup::new()
        .title("GitHub, GitLab and Azure DevOps")
        .description("A connected account lets Vekro list your repositories to clone, list and open pull requests, and gives Git the token for that host over HTTPS. Tokens are kept in the macOS Keychain.")
        .item(SettingItem::render(move |_, _, _| accounts.clone())),
    ),
    SettingPage::new("OpenCommit").resettable(false).group(
      SettingGroup::new().title("Commit messages").item(
        SettingItem::new(
          "Generate commit messages",
          SettingField::render(move |_, _, _| {
            let store = commit_store.clone();
            Button::new("open-commit-settings").label("Configure…").small().outline().on_click(move |_, _, cx| store.emit(CommitSettingsRequested, cx))
          }),
        )
        .description("The sparkle button beside a commit's description writes the summary and description with OpenCommit. Choose the model, language and message style here."),
      ),
    ),
    // One titled group: with two, the Kit adds an arrow and a submenu to the page in the list on the left.
    SettingPage::new("Updates").resettable(false).group(
      SettingGroup::new()
        .title("Vekro")
        .item(SettingItem::render(move |_, _, _| updates.clone()))
        .item(
          SettingItem::new(
            "Check for updates on launch",
            SettingField::switch(
              move |cx: &App| updates_get.preferences(cx).check_updates,
              move |value: bool, cx: &mut App| updates_set.update(cx, |preferences| preferences.check_updates = value),
            ),
          )
          .description("Vekro asks GitHub (api.github.com) for the latest release of its repository. Nothing about you or your projects is sent. An update is only downloaded when you choose to."),
        ),
    ),
    SettingPage::new("About").resettable(false).group(
      SettingGroup::new()
        .title("Vekro")
        .item(SettingItem::new("Version", SettingField::render(|_, _, _| Label::new(env!("CARGO_PKG_VERSION")).text_sm())))
        .item(SettingItem::new(
          "Repository",
          SettingField::render(|_, _, cx| Label::new(vekro_updater::repository_slug().unwrap_or("—")).text_sm().text_color(cx.theme().muted_foreground)),
        ))
        .item(SettingItem::new(
          "Data folder",
          SettingField::render(|_, _, cx| Label::new(crate::components::display_path(&data_dir())).text_sm().text_color(cx.theme().muted_foreground)),
        )),
    ),
  ])
}
