//! The profile picker and profile management in the workspace.

use super::*;
use crate::profiles::{other_open_profiles, profile_list, show_profile, update_profiles};
use crate::settings::release_settings;
use crate::SwitchProfile;
use gpui_kit::component::avatar::Avatar;
use ws_hosting::Accounts;
use ws_storage::delete_profile_files;

/// The avatar of the profile in the activity bar: between the Kit's small (24 px) and medium (48 px, wider than the button). It is the
/// small avatar stretched to this size: a custom `Size` makes the Kit put the letter in a box half that size, off the center.
const PROFILE_AVATAR_SIZE: Pixels = px(28.);

impl Workspace {
  /// The profile this window shows.
  pub(crate) fn profile(&self) -> &Profile {
    &self.profile
  }

  /// The handle that gives the keyboard to the window itself, where its shortcuts are handled.
  pub(crate) fn focus_handle(&self) -> &FocusHandle {
    &self.focus_handle
  }

  /// The current profile's avatar and the actions from the Profiles menu.
  pub(super) fn render_profile_picker(&self, cx: &App) -> impl IntoElement {
    let palette = crate::theme::colors(cx);
    let current = self.profile.id.clone();
    let focus = self.focus_handle.clone();
    let label = SharedString::from(format!("Profile: {}", self.profile.name));
    // Only the first letter: the avatar takes two letters from a single word.
    let initial: SharedString = self.profile.name.trim().chars().take(1).flat_map(char::to_uppercase).collect::<String>().into();
    Button::new("profile-picker")
      .custom(
        ButtonCustomVariant::new(cx)
          .color(gpui_kit::transparent_black())
          .foreground(palette.rail_text)
          .hover(palette.rail_hover)
          .active(palette.rail_hover),
      )
      .size_10()
      .p_0()
      .rounded_md()
      .accessibility_label(label.clone())
      .tooltip(label)
      .child(Avatar::new().name(initial).small().size(PROFILE_AVATAR_SIZE))
      .dropdown_menu_with_anchor(Anchor::LeftCenter, move |mut menu, _, cx| {
        menu = menu.action_context(focus.clone());
        for profile in profile_list(cx).profiles {
          let checked = current == profile.id;
          menu = menu.menu_with_check(profile.name, checked, Box::new(SwitchProfile { id: profile.id }));
        }
        menu.separator().menu("Edit…", Box::new(EditProfiles)).menu("Add Profile…", Box::new(AddProfile))
      })
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
        window.set_window_title(&format!("{} — {name}", ws_product::NAME));
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
