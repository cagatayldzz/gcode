//! The Settings dialog, built with the Kit's `Settings` component, and the model that holds the settings.

use crate::updates::{UpdateState, UPDATES_ENABLED};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    switch::Switch,
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
  },
  *,
};
use std::collections::HashMap;
use ws_commitgen::Config;
use ws_hosting::Accounts;
use ws_storage::{Preferences, Profile};

/// The settings of one profile, shared by every window that shows it. Changing `preferences` and notifying makes each of those
/// windows save them and redraw.
pub(crate) struct SettingsModel {
  /// The profile whose files these settings are read from and saved to.
  pub profile: Profile,
  pub preferences: Preferences,
  /// The commit message settings (`commit.json`) that commit message generation uses.
  pub commitgen: Config,
  /// The connected hosting accounts (`accounts.json`); their tokens are in the Keychain.
  pub accounts: Accounts,
  /// The app's own updates.
  pub updates: UpdateState,
  /// The accounts whose permissions are being checked, by key.
  pub rechecking: Vec<String>,
}

/// The buttons of the activity bar, by id and name, in their default order.
pub(crate) const ACTIVITY_ITEMS: [(&str, &str); 6] =
  [("explorer", "Explorer"), ("search", "Search"), ("git", "Git"), ("extensions", "Extensions"), ("terminal", "Terminal"), ("agents", "AI agents")];

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
pub(crate) fn activity_row(store: &Store, index: usize, count: usize, id: &'static str, shown: bool, cx: &App) -> impl IntoElement {
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
    commitgen: Config::load_or_import(),
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
  pub(crate) fn preferences(&self, cx: &App) -> Preferences {
    self.0.read(cx).preferences.clone()
  }

  /// Changes one setting and tells the windows.
  pub(crate) fn update(&self, cx: &mut App, change: impl FnOnce(&mut Preferences)) {
    self.0.update(cx, |model, cx| {
      change(&mut model.preferences);
      cx.notify();
    });
  }
}

/// The size of code in the editor and in changes.
pub(crate) fn code_font_size(model: &Entity<SettingsModel>, cx: &App) -> Pixels {
  px(model.read(cx).preferences.code_font_size as f32)
}

/// Asks for a folder and makes it the projects folder.
pub(crate) fn choose_projects_folder(store: Store, cx: &mut App) {
  let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
  cx.spawn(async move |cx| {
    let Ok(Ok(Some(paths))) = picked.await else { return };
    let Some(path) = paths.into_iter().next() else { return };
    cx.update(|cx| store.update(cx, |preferences| preferences.projects_dir = Some(path)));
  })
  .detach();
}
