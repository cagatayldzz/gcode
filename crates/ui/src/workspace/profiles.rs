//! Profiles in the window: the name beside the title bar, and creating, renaming and deleting them.

use super::*;
use crate::profiles::{other_open_profiles, profile_list, show_profile, update_profiles};
use crate::settings::release_settings;
use vekro_hosting::Accounts;
use vekro_storage::delete_profile_files;

impl Workspace {
  /// The profile this window shows.
  pub(crate) fn profile(&self) -> &Profile {
    &self.profile
  }

  /// The name of the profile in the title bar, when there is more than one to tell apart. It opens the Profiles page of Settings.
  pub(super) fn render_profile_badge(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    if profile_list(cx).profiles.len() < 2 {
      return None;
    }
    Some(
      Button::new("title-profile")
        .small()
        .ghost()
        .icon(Icon::empty().path("icons/user.svg"))
        .label(SharedString::from(self.profile.name.clone()))
        .tooltip("Profile: manage profiles")
        .on_click(cx.listener(|this, _, window, cx| this.open_settings_page("Profiles", window, cx))),
    )
  }

  /// Profiles → Edit…: Settings on the Profiles page.
  pub(super) fn edit_profiles(&mut self, _: &EditProfiles, window: &mut Window, cx: &mut Context<Self>) {
    self.open_settings_page("Profiles", window, cx);
  }

  /// Profiles → Add Profile…: asks for a name, creates the profile and opens a window with it.
  pub(super) fn add_profile(&mut self, _: &AddProfile, window: &mut Window, cx: &mut Context<Self>) {
    self.ask_name(
      "New profile",
      "Profile name",
      "A profile keeps its own projects, settings and connected accounts.",
      "",
      window,
      cx,
      |this, name, window, cx| {
        let profile = update_profiles(cx, |list| list.add(name)).ok_or("Enter a name for the profile.")?;
        this.sync_menus(true, cx);
        // Settings may be open under the dialog; the new window comes forward after this update.
        window.close_all_dialogs(cx);
        let id = profile.id;
        cx.defer(move |cx| show_profile(id, cx));
        Ok(())
      },
    );
  }

  /// Asks for a new name for a profile.
  pub(crate) fn rename_profile(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
    let Some(profile) = profile_list(cx).get(&id).cloned() else { return };
    self.ask_name("Rename profile", "Profile name", "", &profile.name, window, cx, move |this, name, window, cx| {
      if name.trim().is_empty() {
        return Err("Enter a name for the profile.".to_string());
      }
      update_profiles(cx, |list| list.rename(&id, name));
      if this.profile.id == id {
        let name = name.trim();
        this.profile.name = name.to_string();
        window.set_window_title(&format!("Vekro — {name}"));
      }
      this.sync_menus(true, cx);
      cx.notify();
      Ok(())
    });
  }

  /// Asks before deleting a profile, its settings and its connected accounts' tokens. A profile with an open window stays.
  pub(crate) fn confirm_delete_profile(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
    let Some(profile) = profile_list(cx).get(&id).cloned() else { return };
    if profile.is_default() {
      return;
    }
    if self.profile.id == id || other_open_profiles(cx).contains(&id) {
      window.push_notification(Notification::error(format!("Close the windows of {} before deleting it.", profile.name)), cx);
      return;
    }
    let (title, workspace) = (SharedString::from(format!("Delete {}?", profile.name)), cx.entity());
    window.open_alert_dialog(cx, move |alert, _, _| {
      let (workspace, id) = (workspace.clone(), id.clone());
      alert
        .title(title.clone())
        .description("Its project list, settings and connected accounts are deleted. The projects' folders stay on disk.")
        .width(px(430.))
        .confirm()
        .ok_text("Delete")
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, window, cx| {
          workspace.update(cx, |this, cx| this.delete_profile(&id, window, cx));
          true
        })
    });
  }

  fn delete_profile(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
    let Some(removed) = update_profiles(cx, |list| list.remove(id)) else { return };
    release_settings(&removed.id, cx);
    self.sync_menus(true, cx);
    cx.notify();
    // Its tokens leave the Keychain with the files, off the interface thread.
    let work = cx.background_executor().spawn(async move {
      let accounts = Accounts::load_in(&removed.accounts_path(), removed.keychain_namespace());
      let keychain = accounts.keychain();
      for account in &accounts.items {
        keychain.delete(&account.key());
      }
      delete_profile_files(&removed).map_err(|error| format!("Could not delete the files of {}: {error}", removed.name))
    });
    cx.spawn_in(window, async move |this, cx| {
      if let Err(message) = work.await {
        let _ = this.update_in(cx, |_, window, cx| window.push_notification(Notification::error(message), cx));
      }
    })
    .detach();
  }
}
