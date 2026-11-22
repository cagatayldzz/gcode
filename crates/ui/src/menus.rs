//! Native menus for the menu bar and Dock icon.

use crate::{
  AddProfile, AddProject, CheckForUpdates, CloseTab, EditProfiles, GoToFile, NewClaude, NewCodex, OpenCommandPalette, OpenSettings, Quit, SaveFile,
  SwitchProfile, ToggleAgents, ToggleTerminal, ViewOnHost,
};
use gpui_kit::*;
use vekro_storage::Profile;

fn profile_item(profile: &Profile) -> MenuItem {
  MenuItem::action(profile.name.clone(), SwitchProfile { id: profile.id.clone() })
}

pub(crate) fn dock_menu(profiles: &[Profile]) -> Vec<MenuItem> {
  [MenuItem::action("Profiles", NoAction).disabled(true)].into_iter().chain(profiles.iter().map(profile_item)).collect()
}

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
      MenuItem::action("Add Project…", AddProject),
      MenuItem::separator(),
      MenuItem::action("Go to File…", GoToFile),
      MenuItem::separator(),
      MenuItem::action("Save", SaveFile),
      MenuItem::action("Close Tab", CloseTab),
    ]),
    Menu::new("View").items([
      MenuItem::action("Command Palette…", OpenCommandPalette),
      MenuItem::separator(),
      MenuItem::action("Toggle Terminal", ToggleTerminal),
      MenuItem::action("Toggle AI Agents", ToggleAgents),
      MenuItem::separator(),
      MenuItem::action("New Codex Session", NewCodex),
      MenuItem::action("New Claude Session", NewClaude),
    ]),
    Menu::new("Repository").items([MenuItem::action(view, ViewOnHost).disabled(host.is_none())]),
    Menu::new("Profiles").items(profiles.iter().map(|profile| profile_item(profile).checked(profile.id == current)).chain([
      MenuItem::separator(),
      MenuItem::action("Edit…", EditProfiles),
      MenuItem::action("Add Profile…", AddProfile),
    ])),
  ]
}
