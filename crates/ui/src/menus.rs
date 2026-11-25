//! Native menus for the menu bar and Dock icon.

use crate::{
  AddProfile, AddProject, CheckForUpdates, CloseTab, EditProfiles, GoToFile, NewClaude, NewCodex, OpenCommandPalette, OpenRecentFile,
  OpenRecentProject, OpenSettings, Quit, SaveFile, SwitchProfile, ToggleAgents, ToggleTerminal, ViewOnHost,
};
use gpui_kit::*;
use ws_storage::{Profile, RecentEntry, RecentList};

fn profile_item(profile: &Profile) -> MenuItem {
  MenuItem::action(profile.name.clone(), SwitchProfile { id: profile.id.clone() })
}

/// How many recent files and projects the Dock menu lists together, and the fewest projects it shows when there are that many.
const DOCK_RECENT: usize = 10;
const DOCK_MIN_PROJECTS: usize = 5;

fn name_of(entry: &RecentEntry) -> String {
  entry.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| entry.path.display().to_string())
}

pub(crate) fn dock_menu(profiles: &[Profile], recent: &RecentList) -> Vec<MenuItem> {
  // What is gone from the disk is not offered.
  let present = |entries: &[RecentEntry]| entries.iter().filter(|entry| entry.path.exists()).cloned().collect::<Vec<_>>();
  let present = RecentList { files: present(&recent.files), projects: present(&recent.projects) };
  let (files, projects) = present.menu_entries(DOCK_RECENT, DOCK_MIN_PROJECTS);
  let files: Vec<MenuItem> = files
    .iter()
    .map(|entry| {
      MenuItem::action(
        format!("\u{1F4C4}  {}", name_of(entry)),
        OpenRecentFile { profile: entry.profile.clone(), path: entry.path.to_string_lossy().into_owned() },
      )
    })
    .collect();
  let projects: Vec<MenuItem> = projects
    .iter()
    .map(|entry| {
      MenuItem::action(
        format!("\u{1F4C1}  {}", name_of(entry)),
        OpenRecentProject { profile: entry.profile.clone(), path: entry.path.to_string_lossy().into_owned() },
      )
    })
    .collect();
  let mut menu = Vec::new();
  for group in [files, projects] {
    if !group.is_empty() {
      menu.extend(group);
      menu.push(MenuItem::separator());
    }
  }
  menu.push(MenuItem::action("Profiles", NoAction).disabled(true));
  menu.extend(profiles.iter().map(profile_item));
  menu
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
    Menu::new(ws_product::NAME).items([
      MenuItem::action("Settings…", OpenSettings),
      MenuItem::action("Check for Updates…", CheckForUpdates),
      MenuItem::separator(),
      MenuItem::action(format!("Quit {}", ws_product::NAME), Quit),
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
