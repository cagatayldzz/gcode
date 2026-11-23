//! The interface: projects, editor, terminal and Git in one window, built with GPUI Kit.

mod agents;
mod all_changes;
mod assets;
mod change_view;
mod close_order;
mod command_palette;
mod commit_settings;
mod commit_view;
mod components;
mod connect_account;
mod connections;
mod daemon;
mod diff_view;
mod editor;
mod explorer;
mod find_bar;
mod job_scope;
mod labels;
mod line_comments;
mod mention_picker;
mod menus;
mod merge_dialog;
mod minimap;
mod occurrences;
mod profiles;
mod project_manager;
mod project_search;
mod project_settings;
mod project_setup;
mod pull_request;
mod quick_open;
mod repository_picker;
mod selection;
mod settings;
mod settings_window;
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
  app,
  [
    Quit,
    AddProject,
    OpenSettings,
    CheckForUpdates,
    SearchProject,
    // Go to file: the dialog that opens a file by a few letters of its path.
    GoToFile,
    // The list of every command and project, searchable.
    OpenCommandPalette,
    OpenSearchResults,
    NewTerminal,
    SaveFile,
    // In the editor: comment out, comment in, or switch the comments of the selected lines.
    AddLineComment,
    RemoveLineComment,
    ToggleLineComment,
    // In the editor: select the next place the selected text (or the word at the cursor) occurs, keeping the selections so far.
    SelectNextOccurrence,
    // In the editor: the find widget, and the same with the replace row open.
    FindInFile,
    ReplaceInFile,
    CloseTab,
    // In a terminal: close the pane that has the keyboard. Only from menus: ⌘W never closes a terminal.
    ClosePane,
    MentionProjects,
    ToggleTerminal,
    ToggleAgents,
    // The activity bar's project settings button: the settings of the open project take the main area, or give it back.
    ToggleProjectSettings,
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
#[action(namespace = app, no_json)]
pub struct SwitchProfile {
  pub id: String,
}

/// Moves data saved under the previous name, loads the bundled fonts and registers editor highlighting.
pub fn init(cx: &mut App) {
  // Data saved under the product's previous name moves to the current folder before anything reads it.
  if let Err(error) = ws_storage::migrate_legacy_data_dir() {
    eprintln!("Could not move the data folder of the previous name: {error}");
  }
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
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
    std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")),
  ]) {
    eprintln!("Could not load fonts: {error}");
  }
}
