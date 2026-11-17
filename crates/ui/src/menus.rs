//! The menu bar of the application: only what is needed, with the shortcuts the actions already have.

use crate::{AddProject, CloseTab, NewWindow, OpenSettings, Quit, SaveFile, ToggleTerminal, ViewOnHost};
use gpui_kit::*;

/// The menus. `host` names the service of the open project's remote ("GitHub", "GitLab", "Azure DevOps", or a host name): the
/// Repository menu offers to open the repository there; without a remote that item is disabled.
pub fn menus(host: Option<&str>) -> Vec<Menu> {
  let view = match host {
    Some(host) => format!("View on {host}"),
    None => "View on Host".to_string(),
  };
  vec![
    Menu::new("Vekro").items([MenuItem::action("Settings…", OpenSettings), MenuItem::separator(), MenuItem::action("Quit Vekro", Quit)]),
    Menu::new("File").items([
      MenuItem::action("New Window", NewWindow),
      MenuItem::action("Add Project…", AddProject),
      MenuItem::separator(),
      MenuItem::action("Save", SaveFile),
      MenuItem::action("Close Tab", CloseTab),
    ]),
    Menu::new("View").items([MenuItem::action("Toggle Terminal", ToggleTerminal)]),
    Menu::new("Repository").items([MenuItem::action(view, ViewOnHost).disabled(host.is_none())]),
  ]
}
