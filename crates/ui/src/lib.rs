//! The Vekro interface: projects, editor, terminal and Git in one window, built with GPUI Kit.

mod agents;
mod all_changes;
mod assets;
mod change_view;
mod close_order;
mod commit_settings;
mod commit_view;
mod components;
mod connect_account;
mod connections;
mod diff_view;
mod editor;
mod explorer;
mod mention_picker;
mod menus;
mod merge_dialog;
mod minimap;
mod profiles;
mod project_manager;
mod project_search;
mod project_settings;
mod project_setup;
mod pull_request;
mod repository_picker;
mod selection;
mod settings;
mod syntax;
mod terminal;
mod terminal_layout;
mod theme;
mod updates;
mod workspace;

use gpui_kit::*;

pub use assets::Assets;
pub use menus::menus;
pub use profiles::{open_last_window, open_window};
pub use workspace::{Workspace, TOOLBAR_HEIGHT};

actions!(
  vekro,
  [
    Quit,
    AddProject,
    OpenSettings,
    CheckForUpdates,
    SearchProject,
    OpenSearchResults,
    NewTerminal,
    SaveFile,
    CloseTab,
    MentionProjects,
    ToggleTerminal,
    ToggleAgents,
    // The AI view: a new Codex or Claude session in the open project.
    NewCodex,
    NewClaude,
    ToggleAllChanges,
    // Terminal panes: split side by side, split one above the other.
    SplitRight,
    SplitDown,
    // In a terminal: copy the selection, paste, clear the history.
    CopySelection,
    PasteClipboard,
    ClearBuffer,
    // Repository menu: open the repository on the service that hosts it.
    ViewOnHost,
    // Profiles menu: ask for a name and open a window with a new profile; open Settings on the Profiles page.
    AddProfile,
    EditProfiles
  ]
);

/// Profiles menu: show the window of the profile with this id, or open one.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = vekro, no_json)]
pub struct SwitchProfile {
  pub id: String,
}

/// Loads the bundled fonts and registers editor highlighting.
pub fn init(cx: &mut App) {
  syntax::register_highlights();
  profiles::init(cx);
  if let Err(error) = cx.text_system().add_fonts(vec![
    // Static weights: the text system picks a weight by font file, so a variable font would draw every weight as regular.
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Geist-Regular.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Geist-Italic.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Geist-Medium.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Geist-SemiBold.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Geist-Bold.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono-Regular.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono-Medium.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono-SemiBold.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/GeistMono-Bold.ttf")),
  ]) {
    eprintln!("Could not load fonts: {error}");
  }
}
