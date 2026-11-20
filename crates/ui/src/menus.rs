//! The menu bar of the application: only what is needed, with the shortcuts the actions already have.

use crate::{
  AddProfile, AddProject, CheckForUpdates, CloseTab, EditProfiles, NewClaude, NewCodex, NewWindow, OpenSettings, Quit, SaveFile, SwitchProfile,
  ToggleAgents, ToggleTerminal, ViewOnHost,
};
use gpui_kit::*;
use vekro_storage::Profile;

/// The menus. `host` names the service of the open project's remote ("GitHub", "GitLab", "Azure DevOps", or a host name): the
/// Repository menu offers to open the repository there; without a remote that item is disabled.
/// The Profiles menu lists `profiles`, with a check by `current`, the one the window in front shows.
pub fn menus(host: Option<&str>, profiles: &[Profile], current: &str) -> Vec<Menu> {
  let view = match host {
    Some(host) => format!("View on {host}"),
    None => "View on Host".to_string(),
  };
  vec![
    Menu::new("Vekro").items([
      MenuItem::action("Settings…", OpenSettings),
      MenuItem::action("Check for Updates…", CheckForUpdates),
      MenuItem::separator(),
      MenuItem::action("Quit Vekro", Quit),
    ]),
    Menu::new("File").items([
      MenuItem::action("New Window", NewWindow),
      MenuItem::action("Add Project…", AddProject),
      MenuItem::separator(),
      MenuItem::action("Save", SaveFile),
      MenuItem::action("Close Tab", CloseTab),
    ]),
    Menu::new("View").items([
      MenuItem::action("Toggle Terminal", ToggleTerminal),
      MenuItem::action("Toggle AI Agents", ToggleAgents),
      MenuItem::separator(),
      MenuItem::action("New Codex Session", NewCodex),
      MenuItem::action("New Claude Session", NewClaude),
    ]),
    Menu::new("Repository").items([MenuItem::action(view, ViewOnHost).disabled(host.is_none())]),
    Menu::new("Profiles").items(
      profiles
        .iter()
        .map(|profile| MenuItem::action(profile.name.clone(), SwitchProfile { id: profile.id.clone() }).checked(profile.id == current))
        .chain([MenuItem::separator(), MenuItem::action("Edit…", EditProfiles), MenuItem::action("Add Profile…", AddProfile)]),
    ),
  ]
}
