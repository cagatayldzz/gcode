//! The files and projects used lately, for the Dock menu: the files edited last on top, the projects opened last below them.

use crate::{profiles, OpenRecentFile, OpenRecentProject};
use gpui_kit::*;
use std::path::{Path, PathBuf};
use ws_storage::RecentList;

/// The recent list, loaded when first needed. A change is written to `recent.json` at once.
struct RecentGlobal(RecentList);

impl Global for RecentGlobal {}

fn ensure_loaded(cx: &mut App) {
  if !cx.has_global::<RecentGlobal>() {
    cx.set_global(RecentGlobal(RecentList::load(&RecentList::default_path())));
  }
}

/// Applies `change`; when it changed the list, saves it (a failed save is logged and the change stays for this run) and rebuilds the Dock menu.
fn update(cx: &mut App, change: impl FnOnce(&mut RecentList)) {
  ensure_loaded(cx);
  let list = &mut cx.global_mut::<RecentGlobal>().0;
  let before = list.clone();
  change(list);
  if *list == before {
    return;
  }
  if let Err(error) = list.save(&RecentList::default_path()) {
    eprintln!("Could not save the recent files and projects: {error}");
  }
  refresh_dock_menu(cx);
}

/// A file of the profile was edited and saved.
pub(crate) fn record_file(profile: &str, path: &Path, cx: &mut App) {
  update(cx, |list| list.touch_file(profile, path));
}

/// A project of the profile was opened.
pub(crate) fn record_project(profile: &str, path: &Path, cx: &mut App) {
  update(cx, |list| list.touch_project(profile, path));
}

/// A project left its profile, or was found missing.
pub(crate) fn forget_project(profile: &str, path: &Path, cx: &mut App) {
  update(cx, |list| list.forget_project(profile, path));
}

/// A profile was deleted.
pub(crate) fn forget_profile(profile: &str, cx: &mut App) {
  update(cx, |list| list.forget_profile(profile));
}

/// Builds the Dock menu from the profiles and the recent list again.
pub(crate) fn refresh_dock_menu(cx: &mut App) {
  ensure_loaded(cx);
  let profiles = profiles::profile_list(cx).profiles;
  let menu = crate::menus::dock_menu(&profiles, &cx.global::<RecentGlobal>().0);
  cx.set_dock_menu(menu);
}

/// Registers the commands of the recent entries in the Dock menu.
pub(crate) fn init(cx: &mut App) {
  cx.on_action(|action: &OpenRecentFile, cx| {
    let (profile, path) = (action.profile.clone(), PathBuf::from(&action.path));
    cx.defer(move |cx| profiles::open_recent(&profile, path, true, cx));
  });
  cx.on_action(|action: &OpenRecentProject, cx| {
    let (profile, path) = (action.profile.clone(), PathBuf::from(&action.path));
    cx.defer(move |cx| profiles::open_recent(&profile, path, false, cx));
  });
}
