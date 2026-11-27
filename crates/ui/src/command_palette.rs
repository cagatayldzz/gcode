//! The command palette (⇧⌘P): every command of the menus and the projects in one searchable list.
//!
//! A command is an `Action`, the same one a button, a menu item or a key runs, so the palette adds no command of its own.

use crate::{
  components::display_path, theme::FONT_SIZE_SM, AddLineComment, AddProfile, AddProject, CheckForUpdates, CloseTab, DisableProjectLanguageServers,
  EditProfiles, EnableProjectLanguageServers, FindInFile, GoToFile, NewClaude, NewCodex, NewTerminal, OpenSettings, RemoveLineComment, ReplaceInFile,
  SaveFile, SearchProject, SelectNextOccurrence, ShowShortcuts, SplitDown, SplitRight, ToggleAgents, ToggleAllChanges, ToggleLineComment,
  ToggleProjectSettings, ToggleTerminal, ViewOnHost,
};
use gpui_kit::{
  component::{
    command::{Command, CommandGroup, CommandItem, CommandState},
    h_flex,
    kbd::Kbd,
    label::Label,
    v_flex, ActiveTheme as _,
  },
  *,
};
use std::path::{Path, PathBuf};

/// What choosing an entry does.
pub(crate) enum PaletteCommand {
  /// Runs the action where the window's focus is.
  Action(Box<dyn Action>),
  /// Opens the project.
  Project(PathBuf),
}

impl Clone for PaletteCommand {
  fn clone(&self) -> Self {
    match self {
      Self::Action(action) => Self::Action(action.boxed_clone()),
      Self::Project(path) => Self::Project(path.clone()),
    }
  }
}

/// Sent with the command the user chose.
pub(crate) struct CommandChosen(pub PaletteCommand);

impl EventEmitter<CommandChosen> for CommandPalette {}

struct Entry {
  label: SharedString,
  /// Shown dimmed after the label.
  detail: SharedString,
  keywords: &'static [&'static str],
  command: PaletteCommand,
  /// The open project, marked with a check.
  current: bool,
}

pub(crate) struct CommandPalette {
  commands: Vec<Entry>,
  projects: Vec<Entry>,
  state: Entity<CommandState>,
}

/// The commands that have a menu item or a key, with the words they are also found by.
fn commands() -> Vec<Entry> {
  fn entry(label: &'static str, keywords: &'static [&'static str], action: impl Action) -> Entry {
    Entry { label: label.into(), detail: SharedString::default(), keywords, command: PaletteCommand::Action(Box::new(action)), current: false }
  }
  vec![
    entry("Go to File", &["open", "quick", "find file"], GoToFile),
    entry("Search in Project", &["find", "grep", "replace"], SearchProject),
    entry("Add Project", &["open folder", "clone", "new"], AddProject),
    entry("Save File", &["write"], SaveFile),
    entry("Toggle Split Editor", &["editor", "side"], crate::ToggleEditorSplit),
    entry("Toggle Terminal Dock", &["terminal", "bottom", "maximize"], crate::ToggleTerminalDock),
    entry("Format Document", &["formatter", "indent"], crate::FormatDocument),
    entry("Quick Fix", &["code actions", "diagnostics"], gpui_kit::base::input::ToggleCodeActions),
    entry("Add Line Comment", &["comment out", "#", "//"], AddLineComment),
    entry("Remove Line Comment", &["uncomment", "comment in"], RemoveLineComment),
    entry("Toggle Line Comment", &["comment", "uncomment"], ToggleLineComment),
    entry("Find in File", &["search", "replace"], FindInFile),
    entry("Replace in File", &["find", "substitute"], ReplaceInFile),
    entry("Add Next Occurrence", &["select", "multi cursor", "same word"], SelectNextOccurrence),
    entry("Close Tab", &["close file"], CloseTab),
    entry("New Terminal", &["shell", "console"], NewTerminal),
    entry("Toggle Terminal", &["shell", "console", "show", "hide"], ToggleTerminal),
    entry("Keyboard Shortcuts", &["keys", "keymap", "hotkeys", "help"], ShowShortcuts),
    entry("Split Terminal Right", &["pane", "side by side"], SplitRight),
    entry("Split Terminal Down", &["pane", "below"], SplitDown),
    entry("Toggle AI Agents", &["codex", "claude", "show", "hide"], ToggleAgents),
    entry("New Codex Session", &["agent", "ai"], NewCodex),
    entry("New Claude Session", &["agent", "ai"], NewClaude),
    entry("Toggle All Changes", &["git", "diff", "review"], ToggleAllChanges),
    entry("Show Branch Stashes", &["git", "restore", "stash"], crate::ShowBranchStashes),
    entry("Toggle Project Settings", &["index", "project"], ToggleProjectSettings),
    entry("Enable Language Servers for This Project", &["trust", "lsp", "typescript"], EnableProjectLanguageServers),
    entry("Disable Language Servers for This Project", &["revoke", "trust", "lsp"], DisableProjectLanguageServers),
    entry("View Repository on Host", &["github", "gitlab", "browser", "remote"], ViewOnHost),
    entry("Add Profile", &["account", "workspace"], AddProfile),
    entry("Edit Profiles", &["account", "workspace"], EditProfiles),
    entry("Settings", &["preferences", "theme", "font"], OpenSettings),
    entry("Check for Updates", &["version", "upgrade"], CheckForUpdates),
  ]
}

impl CommandPalette {
  /// `projects` are the folders the app knows with their names; `open` is the one shown now.
  pub fn new(projects: Vec<(PathBuf, String)>, open: Option<&Path>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let projects = projects
      .into_iter()
      .map(|(path, name)| Entry {
        label: format!("Open Project: {name}").into(),
        detail: display_path(&path).into(),
        keywords: &["switch", "project"],
        current: open == Some(path.as_path()),
        command: PaletteCommand::Project(path),
      })
      .collect();
    Self { commands: commands(), projects, state: cx.new(|cx| CommandState::new(window, cx)) }
  }

  pub fn focus(&self, window: &mut Window, cx: &mut App) {
    self.state.update(cx, |state, cx| state.focus(window, cx));
  }

  fn choose(&mut self, section: usize, row: usize, cx: &mut Context<Self>) {
    let list = if section == 0 { &self.commands } else { &self.projects };
    if let Some(entry) = list.get(row) {
      cx.emit(CommandChosen(entry.command.clone()));
    }
  }
}

/// A row: the label, what the entry adds, and the keys that run its action as the keymap has them now.
fn item(entry: &Entry, muted: Hsla) -> CommandItem {
  let label = entry.label.clone();
  let detail = entry.detail.clone();
  let action = match &entry.command {
    PaletteCommand::Action(action) => Some(action.boxed_clone()),
    PaletteCommand::Project(_) => None,
  };
  CommandItem::new().label(entry.label.clone()).keywords(entry.keywords.iter().copied()).checked(entry.current).child(move |window, _| {
    h_flex()
      .w_full()
      .gap_2()
      .items_center()
      .justify_between()
      .child(
        h_flex()
          .gap_2()
          .items_baseline()
          .min_w_0()
          .child(Label::new(label.clone()).whitespace_nowrap())
          .child(Label::new(detail.clone()).text_size(FONT_SIZE_SM).text_color(muted).overflow_hidden().whitespace_nowrap().text_ellipsis()),
      )
      .children(action.as_ref().and_then(|action| Kbd::binding_for_action(action.as_ref(), None, window)).map(|keys| keys.appearance(true)))
  })
}

impl Render for CommandPalette {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let this = cx.entity().downgrade();
    let mut palette = Command::new(&self.state)
      .bordered(false)
      .highlight_matches(false)
      .placeholder("Type a command or a project")
      .empty(move |_, _, _| Label::new("No command matches.").text_color(muted).p_2())
      .group(CommandGroup::new().label("Commands").items(self.commands.iter().map(|entry| item(entry, muted))));
    // The sections are the groups present, so a missing project group never shifts the commands.
    if !self.projects.is_empty() {
      palette = palette.group(CommandGroup::new().label("Projects").items(self.projects.iter().map(|entry| item(entry, muted))));
    }
    v_flex().child(palette.on_confirm(move |index, _, cx| {
      let _ = this.update(cx, |this, cx| this.choose(index.section, index.row, cx));
    }))
  }
}
