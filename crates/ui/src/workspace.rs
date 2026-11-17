use crate::{
  change_view::ChangeView,
  commit_settings::{CommitSettings, CommitSettingsSaved},
  commit_view::CommitView,
  components::{avatar_look, display_path, strip, strip_tab},
  connect_account::{AccountConnected, ConnectAccount},
  connections::{accounts_path, credentials_for_repository, describe, AccountsList},
  editor::{ActiveTab, EditorPanel},
  explorer,
  project_manager::ProjectManager,
  project_settings::{AppearanceChosen, ProjectSettings},
  project_setup::{ProjectReady, ProjectSetup},
  pull_request::{PullRequestEvent, PullRequestForm, PullRequestTarget},
  settings::{
    init_settings, settings_view, AccountRecheckRequested, AccountRemoveRequested, AddAllProjectsRequested, CommitSettingsRequested,
    ConnectAccountRequested, SettingsGlobal, SettingsModel,
  },
  terminal::{Terminal, TerminalAttentionChanged, TerminalExited},
  terminal_layout::{Arrangement, Direction, Node},
  theme::{apply_theme, colors, FONT_SIZE, FONT_SIZE_SM},
  updates::{AppUpdate, UpdatePanel},
  AddProject, CloseTab, NewTerminal, OpenSettings, SaveFile, SplitDown, SplitRight, ToggleTerminal, ViewOnHost,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonCustomVariant, ButtonVariant, ButtonVariants as _},
    checkbox::Checkbox,
    dock::{panel_handle, DockArea, DockLayout, DockSkin},
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    label::Label,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    popover::Popover,
    resizable::{h_resizable, resizable_panel, v_resizable},
    scroll::{ScrollableElement as _, Scrollbar, ScrollbarAxis},
    spinner::Spinner,
    status_bar::StatusBar,
    tab::{Tab, TabBar},
    tooltip::Tooltip,
    v_flex, ActiveTheme, Disableable as _, Icon, IconName, Sizable as _, TitleBar, WindowExt as _,
  },
  *,
};
use std::{
  collections::{HashMap, HashSet},
  ops::Range,
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
  time::Duration,
};
use vekro_git::{BranchChoices, BranchRef, CommitInfo, Comparison, Summary};
use vekro_hosting::{Connection, KeychainStore, PullRequest, RepoRef, TokenStore as _};
use vekro_opencommit::DEFAULT_INSTRUCTIONS;
use vekro_storage::{Category, Preferences, Project, ProjectList, WindowMode, WindowPlacement};

/// Height of the top band: the traffic lights and the title.
pub const TOOLBAR_HEIGHT: Pixels = px(40.);

/// The count on the Git button: how small its digits are, and how far it reaches past the button's right edge.
const BADGE_FONT_SIZE: Pixels = px(10.);
const BADGE_OVERHANG: Pixels = px(4.);

/// The width of the bar at the far left that chooses the view of the sidebar.
const ACTIVITY_BAR_WIDTH: Pixels = px(56.);

/// The width of the branch menu and of a category's project list.
const MENU_WIDTH: Pixels = px(280.);

/// The width of the project menu: wider, as each project also shows what is waiting in it.
const PROJECT_MENU_WIDTH: Pixels = px(380.);

/// How tall the branch list and a category's project list get before they scroll.
const MENU_LIST_MAX_HEIGHT: Pixels = px(320.);

/// How often the toolbar redraws while git reports its progress.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

/// How long the pointer must rest on a file in the Explorer before it is prepared for opening, so that moving past
/// files costs nothing.
const HOVER_PREPARE_DELAY: Duration = Duration::from_millis(120);

/// How often the Git summary and the folder tree are read again while the window is open.
const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// Width of the sidebar when the window opens.
const SIDEBAR_WIDTH: Pixels = px(280.);

/// How narrow and how wide the sidebar can be dragged.
const SIDEBAR_MIN_WIDTH: Pixels = px(200.);
const SIDEBAR_MAX_WIDTH: Pixels = px(960.);

/// Indentation of each tree level.
const INDENT: Pixels = px(16.);

/// The two views of the sidebar, in tab order.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum SidebarTab {
  #[default]
  Explorer,
  Git,
}

impl SidebarTab {
  const ALL: [SidebarTab; 2] = [SidebarTab::Explorer, SidebarTab::Git];

  fn icon(self) -> Icon {
    match self {
      SidebarTab::Explorer => Icon::empty().path("icons/folder-tree.svg"),
      SidebarTab::Git => Icon::empty().path("icons/git-branch.svg"),
    }
  }

  fn label(self) -> &'static str {
    match self {
      SidebarTab::Explorer => "Explorer",
      SidebarTab::Git => "Git",
    }
  }
}

/// What the project menu shows beside a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectStatus {
  /// Files with uncommitted changes.
  changed: usize,
  /// Commits are waiting to be pushed.
  push: bool,
}

/// What the window shows for a project that is not open at the moment.
#[derive(Default)]
struct ProjectView {
  expanded: HashSet<PathBuf>,
  selected_entry: Option<PathBuf>,
  selected_change: Option<PathBuf>,
}

/// A push, pull or fetch that is running: what the toolbar button says, and the last progress line git reported.
struct RemoteWork {
  title: SharedString,
  progress: Arc<Mutex<String>>,
}

/// The open pull requests of the repository, as the toolbar's menu knows them.
#[derive(Clone)]
enum PullRequests {
  Idle,
  Loading,
  Loaded(Vec<PullRequest>),
  Failed(SharedString),
}

/// What a host refused, for the warning an account shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum IssueKind {
  Push,
  PullRequests,
  Workflows,
}

/// The two views of the Git tab: the files to commit, and the commits already made.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GitView {
  Changes,
  History,
}

/// Which side of a comparison the History list shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CompareSide {
  /// Commits of the other branch that the current one does not have.
  Behind,
  /// Commits of the current branch that the other one does not have.
  Ahead,
}

/// The History list compared with another branch ("Select Branch to Compare…").
struct CompareView {
  branch: String,
  side: CompareSide,
  /// The commits of both sides; `None` while they are being read.
  found: Option<Comparison>,
}

/// How many commits the History view loads at first, and loads more of each time the list is scrolled to its end.
const HISTORY_PAGE: usize = 50;

/// How long the window must rest before its place is written.
const WINDOW_SAVE_DELAY: Duration = Duration::from_millis(500);

/// How long after a scroll the History list is checked for its end.
const HISTORY_SCROLL_SETTLE: Duration = Duration::from_millis(80);

/// How close to the end of the History list counts as its end.
const HISTORY_END_MARGIN: Pixels = px(80.);

/// The two tabs of the branch menu.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BranchTab {
  Branches,
  PullRequests,
}

/// One window: the title bar, the sidebar on the left and the open project on the right.
pub struct Workspace {
  projects: ProjectList,
  /// The settings every window shares.
  settings: Entity<SettingsModel>,
  git: Option<Summary>,
  expanded: HashSet<PathBuf>,
  rows: Vec<explorer::Row>,
  selected_entry: Option<PathBuf>,
  /// The changed file whose diff is open, relative to the repository.
  selected_change: Option<PathBuf>,
  /// The Explorer and Git selections of the projects that are not open.
  views: HashMap<PathBuf, ProjectView>,
  /// The view the sidebar shows.
  sidebar_tab: SidebarTab,
  /// The file in the Explorer the pointer is on.
  hovered_file: Option<PathBuf>,
  focus_handle: FocusHandle,
  _setup_subscription: Option<Subscription>,
  _dialog_subscription: Option<Subscription>,
  /// The scroll position of the folder tree.
  explorer_scroll: UniformListScrollHandle,
  /// The changed files left out of the next commit.
  excluded: HashSet<PathBuf>,
  commit_summary: Entity<InputState>,
  commit_description: Entity<TextareaState>,
  /// A commit is being made.
  committing: bool,
  /// The open pull requests the branch menu lists.
  /// Which tab of the branch menu is shown.
  branch_tab: BranchTab,
  /// The view of the Git tab, the commits it lists, the one chosen and the view of it in the main area.
  git_view: GitView,
  history: Vec<CommitInfo>,
  /// The branch the History list is compared with, if one was chosen.
  compare: Option<CompareView>,
  /// The branches the field offers while it is open; `None` while they are being read.
  compare_choices: Option<BranchChoices>,
  compare_open: bool,
  compare_filter: Entity<InputState>,
  compare_epoch: u64,
  /// A branch is being merged into the current one.
  merging: bool,
  selected_commit: Option<String>,
  commit_view: Entity<CommitView>,
  /// The changes of the chosen file of the Changes view, shown in the main area.
  change_view: Entity<ChangeView>,
  pull_requests: PullRequests,
  /// Which load the listed pull requests belong to, so a slow answer for another project is ignored.
  pr_epoch: u64,
  /// Which read of the history the listed commits belong to.
  history_epoch: u64,
  /// How many commits the History view asks for now, and its scroll position (to load more near the end).
  history_limit: usize,
  history_scroll: ScrollHandle,
  /// The window is the one in front, and what the Repository menu said last.
  window_active: bool,
  menu_host: Option<String>,
  /// The terminals fill the main area (the right part of the window) in place of the editor.
  terminal_open: bool,
  /// A commit message is being generated.
  generating: bool,
  /// What each project holds, read when the project menu opens and while it is open.
  project_status: HashMap<PathBuf, ProjectStatus>,
  /// The push, pull or fetch that is running.
  remote_work: Option<RemoteWork>,
  /// The branch menu is open, with the local branches it lists and the field that filters them.
  branch_menu_open: bool,
  branches: Vec<String>,
  branch_filter: Entity<InputState>,
  /// The repository menu of the title bar is open.
  projects_menu_open: bool,
  project_filter: Entity<InputState>,
  /// Writes where the window is once it has stopped moving or resizing; a newer change replaces (cancels) the pending write.
  window_save: Task<()>,
  _window_bounds: Subscription,
  _commit_subscriptions: Vec<Subscription>,
  _settings_subscriptions: Vec<Subscription>,
  _appearance: Subscription,
  _activation: Subscription,
  /// Repaints the window when the editor changes: the status bar shows the cursor.
  _editor: Subscription,
  /// Selects in the sidebar what the editor area shows.
  _active_tab: Subscription,
  _refresh_task: Task<()>,
  /// A check for projects whose folder was deleted is running.
  syncing: bool,
  dock: Entity<DockArea>,
  editor_panel: Entity<EditorPanel>,
  /// The shells of every project that has been opened, in tabs and split panes, kept alive while another project is shown.
  terminal_tabs: HashMap<PathBuf, Arrangement<Entity<Terminal>>>,
  _terminal_subscriptions: Vec<Subscription>,
}

impl Workspace {
  pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    // The first window loads the settings; later windows share them.
    let settings = match cx.try_global::<SettingsGlobal>() {
      Some(global) => global.0.clone(),
      None => init_settings(cx),
    };
    {
      let preferences = &settings.read(cx).preferences;
      apply_theme(preferences.theme, preferences.code_font_size, window, cx);
    }
    let (dock, _skin) = DockSkin::dock_area("workspace-dock", Some(1), window, cx);
    let editor_panel = cx.new(|cx| EditorPanel::new(window, cx));
    dock.update(cx, |area, cx| {
      area.set_center(DockLayout::h_split().child(DockLayout::tabs().panel_view(panel_handle(editor_panel.clone()), cx), None), window, cx);
    });
    let commit_summary = cx.new(|cx| InputState::new(window, cx).placeholder("Summary (required)"));
    let commit_description = cx.new(|cx| TextareaState::new(window, cx).placeholder("Description"));
    let branch_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find or create a branch"));
    let project_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a project"));
    let compare_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Select Branch to Compare…"));
    let mut this = Self {
      projects: ProjectList::load(&ProjectList::default_path()),
      settings: settings.clone(),
      git: None,
      expanded: HashSet::new(),
      rows: Vec::new(),
      selected_entry: None,
      selected_change: None,
      sidebar_tab: SidebarTab::default(),
      hovered_file: None,
      views: HashMap::new(),
      focus_handle: cx.focus_handle(),
      _setup_subscription: None,
      _dialog_subscription: None,
      explorer_scroll: UniformListScrollHandle::default(),
      excluded: HashSet::new(),
      commit_summary: commit_summary.clone(),
      commit_description: commit_description.clone(),
      committing: false,
      branch_tab: BranchTab::Branches,
      git_view: GitView::Changes,
      history: Vec::new(),
      compare: None,
      compare_choices: None,
      compare_open: false,
      compare_filter: compare_filter.clone(),
      compare_epoch: 0,
      merging: false,
      selected_commit: None,
      commit_view: cx.new(|_| CommitView::new()),
      change_view: cx.new(|_| ChangeView::new()),
      pull_requests: PullRequests::Idle,
      pr_epoch: 0,
      history_epoch: 0,
      history_limit: HISTORY_PAGE,
      history_scroll: ScrollHandle::new(),
      window_active: true,
      menu_host: None,
      terminal_open: false,
      generating: false,
      project_status: HashMap::new(),
      remote_work: None,
      branch_menu_open: false,
      branches: Vec::new(),
      branch_filter: branch_filter.clone(),
      projects_menu_open: false,
      project_filter: project_filter.clone(),
      window_save: Task::ready(()),
      _window_bounds: cx.observe_window_bounds(window, |this, window, cx| this.remember_window(window, cx)),
      _commit_subscriptions: vec![
        // The commit button is enabled once there is a summary.
        cx.subscribe(&commit_summary, |_, _, _: &InputEvent, cx| cx.notify()),
        // The menus list what the filter field matches.
        cx.subscribe(&branch_filter, |_, _, _: &InputEvent, cx| cx.notify()),
        cx.subscribe(&project_filter, |_, _, _: &InputEvent, cx| cx.notify()),
        // Choosing the field lists the branches; typing narrows them; Enter takes the first one.
        cx.subscribe_in(&compare_filter, window, |this, _, event: &InputEvent, window, cx| match event {
          InputEvent::Focus => this.open_compare_list(window, cx),
          InputEvent::PressEnter { .. } => this.choose_first_compare_branch(window, cx),
          _ => cx.notify(),
        }),
      ],
      _settings_subscriptions: vec![
        // Any change to a setting is saved and applied in every window.
        cx.observe_in(&settings, window, |this, settings, window, cx| {
          this.announce_update(window, cx);
          let preferences = settings.read(cx).preferences.clone();
          if let Err(error) = preferences.save(&Preferences::default_path()) {
            eprintln!("Could not save the settings: {error}");
          }
          apply_theme(preferences.theme, preferences.code_font_size, window, cx);
          cx.notify();
        }),
        cx.subscribe_in(&settings, window, |this, _, _: &ConnectAccountRequested, window, cx| {
          if window.is_window_active() {
            this.open_connect_account(window, cx);
          }
        }),
        cx.subscribe_in(&settings, window, |this, _, event: &AccountRecheckRequested, window, cx| {
          if window.is_window_active() {
            this.recheck_account(event.0.clone(), window, cx);
          }
        }),
        cx.subscribe_in(&settings, window, |this, _, event: &AccountRemoveRequested, window, cx| {
          if window.is_window_active() {
            this.remove_account(event.0.clone(), window, cx);
          }
        }),
        cx.subscribe_in(&settings, window, |this, _, _: &CommitSettingsRequested, window, cx| {
          if window.is_window_active() {
            this.open_commit_settings(window, cx);
          }
        }),
        // "Add all" is handled by the window the user is in.
        cx.subscribe_in(&settings, window, |this, _, _: &AddAllProjectsRequested, window, cx| {
          if window.is_window_active() {
            this.add_projects_from_folder(window, cx);
          }
        }),
      ],
      dock,
      editor_panel: editor_panel.clone(),
      terminal_tabs: HashMap::new(),
      _terminal_subscriptions: Vec::new(),
      _appearance: cx.observe_window_appearance(window, |this, window, cx| {
        let preferences = &this.settings.read(cx).preferences;
        apply_theme(preferences.theme, preferences.code_font_size, window, cx)
      }),
      // Changes made in another app (an editor, GitHub Desktop, a terminal) show up as soon as the window is back.
      _editor: cx.observe(&editor_panel, |_, _, cx| cx.notify()),
      _active_tab: cx.subscribe_in(&editor_panel, window, |this, _, shown: &ActiveTab, window, cx| {
        match shown {
          ActiveTab::File(path) => {
            this.selected_entry = Some(path.clone());
            this.selected_change = None;
          }
          ActiveTab::Nothing => {
            this.selected_entry = None;
            this.selected_change = None;
          }
        }
        // Closing the tab that held the focus leaves nothing focused, and then no shortcut (⌘W, ⌘S) reaches the window.
        if window.focused(cx).is_none() {
          this.focus_handle.focus(window, cx);
        }
        cx.notify();
      }),
      _activation: cx.observe_window_activation(window, |this, window, cx| {
        this.window_active = window.is_window_active();
        if window.is_window_active() {
          this.sync_menus(true, cx);
          this.announce_update(window, cx);
          this.refresh_in_background(cx);
          this.sync_projects(window, cx);
        }
      }),
      _refresh_task: cx.spawn_in(window, async move |this, cx| loop {
        cx.background_executor().timer(REFRESH_INTERVAL).await;
        let refreshed = this.update_in(cx, |this, window, cx| {
          this.refresh_in_background(cx);
          this.sync_projects(window, cx);
        });
        if refreshed.is_err() {
          break;
        }
      }),
      syncing: false,
    };
    this.sync_projects(window, cx);
    this.refresh_project();
    this.ensure_terminal(window, cx);
    this
  }

  /// Where new projects go: the chosen folder, else next to the open project, else home.
  fn new_project_dir(&self, cx: &App) -> PathBuf {
    self
      .settings
      .read(cx)
      .preferences
      .projects_dir
      .clone()
      .or_else(|| self.projects.active.as_ref().and_then(|path| path.parent()).map(PathBuf::from))
      .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
      .unwrap_or_default()
  }

  /// The tabs of the open project's shells.
  fn arrangement(&self) -> Option<&Arrangement<Entity<Terminal>>> {
    self.projects.active.as_ref().and_then(|root| self.terminal_tabs.get(root))
  }

  /// Starts a shell in the open project if it has none: a project is not left without one.
  fn ensure_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    if self.terminal_tabs.get(&root).is_none_or(Arrangement::is_empty) {
      if let Some(terminal) = self.start_terminal(root.clone(), window, cx) {
        self.terminal_tabs.entry(root).or_insert_with(Arrangement::new).add_tab(terminal);
      }
    }
  }

  /// Starts a shell in `root`; the caller puts it in a tab or a pane.
  fn start_terminal(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<Terminal>> {
    let started = match Terminal::start(root, cx) {
      Ok(started) => started,
      Err(error) => {
        window.push_notification(Notification::error(format!("Could not start a shell: {error}")), cx);
        return None;
      }
    };
    let terminal = cx.new(|cx| Terminal::new(started, cx));
    self._terminal_subscriptions.push(cx.subscribe_in(&terminal, window, |this, terminal, _: &TerminalExited, window, cx| {
      this.close_pane(terminal.clone(), window, cx);
    }));
    self._terminal_subscriptions.push(cx.subscribe(&terminal, |_, _, _: &TerminalAttentionChanged, cx| cx.notify()));
    Some(terminal)
  }

  /// The shell that has the keyboard, else the first of the shown tab.
  fn focused_terminal(&self, window: &Window, cx: &App) -> Option<Entity<Terminal>> {
    let panes = self.arrangement()?.current()?.panes();
    panes
      .iter()
      .find(|terminal| terminal.read(cx).focus_handle(cx).contains_focused(window, cx))
      .or(panes.first())
      .map(|terminal| (*terminal).clone())
  }

  /// A shell has the keyboard.
  fn terminal_focused(&self, window: &Window, cx: &App) -> bool {
    self.arrangement().is_some_and(|tabs| tabs.panes().iter().any(|terminal| terminal.read(cx).focus_handle(cx).contains_focused(window, cx)))
  }

  /// Gives the keyboard to a shell, once it is drawn.
  fn focus_pane(&self, terminal: &Entity<Terminal>, window: &mut Window, cx: &mut Context<Self>) {
    let handle = terminal.read(cx).focus_handle(cx);
    window.defer(cx, move |window, cx| handle.focus(window, cx));
  }

  /// Gives the keyboard to the shell that should have it.
  fn focus_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(terminal) = self.focused_terminal(window, cx) {
      self.focus_pane(&terminal, window, cx);
    }
  }

  /// Shows the terminals in the main area and puts the keyboard in the shell.
  fn show_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.terminal_open = true;
    self.ensure_terminal(window, cx);
    self.focus_terminal(window, cx);
    cx.notify();
  }

  /// The activity bar's terminal button and its shortcut: the terminals take the main area, or give it back to the editor.
  /// The shells keep running.
  pub fn toggle_terminal(&mut self, _: &ToggleTerminal, window: &mut Window, cx: &mut Context<Self>) {
    if self.terminal_open {
      self.hide_terminal(window, cx);
    } else {
      self.show_terminal(window, cx);
    }
  }

  /// ⌘T: opens a shell in a tab of its own in the open project and shows the terminals.
  pub fn new_terminal(&mut self, _: &NewTerminal, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let Some(terminal) = self.start_terminal(root.clone(), window, cx) else { return };
    self.terminal_tabs.entry(root).or_insert_with(Arrangement::new).add_tab(terminal.clone());
    self.terminal_open = true;
    self.focus_pane(&terminal, window, cx);
    cx.notify();
  }

  /// Splits a pane in two: a new shell beside it or below it, and it gets the keyboard.
  pub(crate) fn split_pane(&mut self, target: Entity<Terminal>, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let Some(terminal) = self.start_terminal(root.clone(), window, cx) else { return };
    let tabs = self.terminal_tabs.entry(root).or_insert_with(Arrangement::new);
    if !tabs.split(&target, terminal.clone(), direction) {
      tabs.add_tab(terminal.clone());
    }
    self.focus_pane(&terminal, window, cx);
    cx.notify();
  }

  /// ⌘D: splits the shell that has the keyboard, side by side.
  pub fn split_right(&mut self, _: &SplitRight, window: &mut Window, cx: &mut Context<Self>) {
    if let (true, Some(terminal)) = (self.terminal_open, self.focused_terminal(window, cx)) {
      self.split_pane(terminal, Direction::Columns, window, cx);
    }
  }

  /// ⇧⌘D: splits the shell that has the keyboard, one above the other.
  pub fn split_down(&mut self, _: &SplitDown, window: &mut Window, cx: &mut Context<Self>) {
    if let (true, Some(terminal)) = (self.terminal_open, self.focused_terminal(window, cx)) {
      self.split_pane(terminal, Direction::Rows, window, cx);
    }
  }

  /// Shows another tab of the terminals.
  pub(crate) fn select_terminal_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(root) = self.projects.active.clone() {
      if let Some(tabs) = self.terminal_tabs.get_mut(&root) {
        tabs.select(index);
      }
    }
    self.focus_terminal(window, cx);
    cx.notify();
  }

  /// Closes a tab of the terminals with all its panes: their shells stop.
  pub(crate) fn close_terminal_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
    let Some(panes) = self.arrangement().and_then(|tabs| tabs.tabs.get(index)).map(|tab| tab.panes().into_iter().cloned().collect::<Vec<_>>()) else {
      return;
    };
    for terminal in panes {
      self.close_pane(terminal, window, cx);
    }
  }

  /// Hides the terminals; the shells keep running. The keyboard goes back to the window, so shortcuts keep working.
  pub(crate) fn hide_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.terminal_open = false;
    self.focus_handle.focus(window, cx);
    cx.notify();
  }

  /// Takes a pane out, because its shell exited or it was closed: the shell stops, a tab left without panes goes, and when the
  /// open project has no shell left the main area goes back to the editor.
  pub(crate) fn close_pane(&mut self, terminal: Entity<Terminal>, window: &mut Window, cx: &mut Context<Self>) {
    for tabs in self.terminal_tabs.values_mut() {
      tabs.remove(&terminal);
    }
    self.terminal_tabs.retain(|_, tabs| !tabs.is_empty());
    if self.terminal_open {
      if self.arrangement().is_some() {
        self.focus_terminal(window, cx);
      } else {
        self.hide_terminal(window, cx);
      }
    }
    cx.notify();
  }

  /// The page of the open project's repository on the service that hosts it, and the service's name.
  fn web_repository(&self, cx: &App) -> Option<vekro_hosting::WebRepository> {
    let url = self.git.as_ref()?.sync.as_ref()?.url.clone();
    vekro_hosting::web_repository(&url, &self.settings.read(cx).accounts.items)
  }

  /// Puts the service of the open project's remote in the Repository menu ("View on GitHub"). Menus belong to the application,
  /// so only the window in front sets them: always when it comes to the front, else only when the service changed.
  fn sync_menus(&mut self, always: bool, cx: &mut Context<Self>) {
    if !self.window_active {
      return;
    }
    let host = self.web_repository(cx).map(|found| found.service);
    if always || host != self.menu_host {
      cx.set_menus(crate::menus(host.as_deref()));
      self.menu_host = host;
    }
  }

  /// Repository → View on GitHub, GitLab or Azure DevOps: opens the repository's page in the browser.
  pub fn view_on_host(&mut self, _: &ViewOnHost, window: &mut Window, cx: &mut Context<Self>) {
    match self.web_repository(cx) {
      Some(found) => cx.open_url(&found.url),
      None => window.push_notification(Notification::error("This project has no remote on a hosting service."), cx),
    }
  }

  /// Opens the Settings dialog.
  pub fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
    self.open_settings_page("General", window, cx);
  }

  /// Marks the account that serves `url` as having a permission problem, because the host refused its token: a warning shows by
  /// Settings until a check finds it fixed. `pull_requests` says which permission was refused.
  fn flag_account_issue(&mut self, url: &str, kind: IssueKind, cx: &mut Context<Self>) {
    self.settings.update(cx, |model, cx| {
      let Some(account) = model.accounts.items.iter_mut().find(|account| RepoRef::for_account(account, url).is_some()) else { return };
      let (action, permission) = match kind {
        IssueKind::PullRequests => ("Opening a pull request", account.provider.pull_request_permission().to_string()),
        IssueKind::Push => ("Pushing or pulling", account.provider.push_permission().to_string()),
        IssueKind::Workflows => {
          ("Pushing commits that change .github/workflows", format!("{} (a classic token, the `workflow` scope)", vekro_git::WORKFLOWS_REFUSED))
        }
      };
      account.issue = Some(format!("{action} was refused with this token. It needs {permission}."));
      if let Err(error) = model.accounts.save(&accounts_path()) {
        eprintln!("Could not save the accounts: {error}");
      }
      cx.notify();
    });
  }

  /// Sets or clears what is wrong with an account's token.
  fn set_account_issue(&mut self, key: &str, issue: Option<String>, cx: &mut Context<Self>) {
    self.settings.update(cx, |model, cx| {
      if let Some(account) = model.accounts.items.iter_mut().find(|account| account.key() == key) {
        account.issue = issue;
      }
      if let Err(error) = model.accounts.save(&accounts_path()) {
        eprintln!("Could not save the accounts: {error}");
      }
      cx.notify();
    });
  }

  /// Asks the service what the account's token may do (see `vekro_hosting::verify_permissions`; nothing is changed) in the open
  /// project's repository if the account serves it, else in the account's first repository. A fixed account loses its warning.
  fn recheck_account(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
    let Some(account) = self.settings.read(cx).accounts.items.iter().find(|account| account.key() == key).cloned() else { return };
    if self.settings.read(cx).rechecking.contains(&key) {
      return;
    }
    self.settings.update(cx, |model, cx| {
      model.rechecking.push(key.clone());
      cx.notify();
    });
    let remote = self.projects.active.as_ref().and_then(|root| vekro_git::remote_url(root));
    let work = cx.background_executor().spawn({
      let account = account.clone();
      async move {
        let token = KeychainStore
          .get(&account.key())
          .ok_or("The token is not in the Keychain any more. Disconnect the account and connect it again.".to_string())?;
        let connection = Connection { account: account.clone(), token };
        let own = remote.and_then(|url| RepoRef::for_account(&account, &url).map(|repository| (repository, url)));
        let (repository, url) = match own {
          Some(found) => found,
          None => {
            let first =
              vekro_hosting::repositories(&connection)?.into_iter().next().ok_or("This account has no repository to check the permissions in.")?;
            let repository = RepoRef::for_account(&account, &first.clone_url).ok_or("Could not tell which repository that is.")?;
            (repository, first.clone_url)
          }
        };
        let report = vekro_hosting::verify_permissions(&connection, &repository, &url);
        let confirmed = report.push.is_some() || report.pull_requests.is_some();
        let problems = if confirmed { report.missing(account.provider) } else { vekro_hosting::check_access(&connection).warnings(account.provider) };
        Ok::<_, String>((problems, confirmed, repository.full_name()))
      }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.settings.update(cx, |model, cx| {
          model.rechecking.retain(|checking| checking != &key);
          cx.notify();
        });
        let label = describe(&account);
        match result {
          Ok((problems, true, repository)) if problems.is_empty() => {
            this.set_account_issue(&key, None, cx);
            window.push_notification(Notification::info(format!("{label}: no permission is missing in {repository}.")), cx);
          }
          Ok((problems, _, _)) if !problems.is_empty() => {
            let issue = format!("Still missing: {}.", problems.join("; "));
            this.set_account_issue(&key, Some(issue.clone()), cx);
            window.push_notification(Notification::warning(format!("{label}. {issue}")), cx);
          }
          Ok(_) => window.push_notification(
            Notification::warning(format!("{label}: the service did not say whether the permissions are enough. Try pushing again.")),
            cx,
          ),
          Err(message) => window.push_notification(Notification::error(message), cx),
        }
      });
    })
    .detach();
  }

  /// Opens Settings on one of its pages ("General", "Accounts", …).
  pub(crate) fn open_settings_page(&mut self, page: &'static str, window: &mut Window, cx: &mut Context<Self>) {
    let fallback = self.new_project_dir(cx);
    let workspace = cx.entity();
    let projects = cx.new(|cx| ProjectManager::new(workspace, cx));
    let accounts = cx.new(AccountsList::new);
    let updates = cx.new(UpdatePanel::new);
    let palette = colors(cx);
    window.open_dialog(cx, move |dialog, _, _| {
      // The dialog's own padding is removed so that the page list reaches the edges of the window.
      dialog.p_0().width(px(820.)).child(
        v_flex()
          .rounded_b_lg()
          .overflow_hidden()
          .child(
            div()
              .border_b_1()
              .border_color(palette.border)
              .child(Label::new("Settings").text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).px_5().py_4()),
          )
          .child(div().h(px(486.)).child(settings_view(
            fallback.clone(),
            projects.clone(),
            accounts.clone(),
            updates.clone(),
            page,
            palette.background,
          ))),
      )
    });
  }

  /// Adds the repositories directly inside the projects folder that are not listed yet, and says how many.
  fn add_projects_from_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let folder = self.new_project_dir(cx);
    let found = vekro_git::find_repositories(&folder);
    let added = self.projects.add_all(found.iter().cloned());
    let message = match (added, found.len()) {
      (_, 0) => format!("No Git repositories in {}.", folder.display()),
      (0, _) => "Every project in that folder is already listed.".to_string(),
      (1, _) => "Added 1 project.".to_string(),
      (count, _) => format!("Added {count} projects."),
    };
    if added > 0 {
      if let Err(error) = self.projects.save(&ProjectList::default_path()) {
        eprintln!("Could not save the project list: {error}");
      }
      // With no project open before, the first one just opened.
      if self.terminal_tabs.is_empty() {
        self.ensure_terminal(window, cx);
        self.refresh_project();
      }
    }
    window.push_notification(Notification::info(message), cx);
    cx.notify();
  }

  /// Remembers where the window is, shortly after it stopped moving or resizing, so that it opens there next time.
  fn remember_window(&mut self, window: &Window, cx: &mut Context<Self>) {
    let (bounds, mode) = match window.window_bounds() {
      WindowBounds::Windowed(bounds) => (bounds, WindowMode::Windowed),
      WindowBounds::Maximized(bounds) => (bounds, WindowMode::Maximized),
      WindowBounds::Fullscreen(bounds) => (bounds, WindowMode::Fullscreen),
    };
    let placement = WindowPlacement {
      x: f32::from(bounds.origin.x),
      y: f32::from(bounds.origin.y),
      width: f32::from(bounds.size.width),
      height: f32::from(bounds.size.height),
      mode,
    };
    self.window_save = cx.spawn(async move |_, cx| {
      cx.background_executor().timer(WINDOW_SAVE_DELAY).await;
      cx.background_executor()
        .spawn(async move {
          if let Err(error) = placement.save(&WindowPlacement::default_path()) {
            eprintln!("Could not save the window size: {error}");
          }
        })
        .await;
    });
  }

  /// Opens the dialog that creates, clones or imports a project.
  pub fn add_project(&mut self, _: &AddProject, window: &mut Window, cx: &mut Context<Self>) {
    let location = self.new_project_dir(cx);
    let accounts = self.settings.read(cx).accounts.clone();
    let setup = cx.new(|cx| ProjectSetup::new(location, accounts, window, cx));
    self._setup_subscription = Some(cx.subscribe_in(&setup, window, |this, _, event: &ProjectReady, window, cx| {
      window.close_all_dialogs(cx);
      let path = event.0.clone();
      this.switch_project(window, cx, move |projects| projects.add(path));
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Add project").width(px(520.)).p_5().child(setup.clone()));
  }

  /// Writes the project list; a failure is logged, the window keeps working.
  fn persist_projects(&self) {
    if let Err(error) = self.projects.save(&ProjectList::default_path()) {
      eprintln!("Could not save the project list: {error}");
    }
  }

  /// Asks for a name and adds an empty category: a place to drop projects on in the project bar.
  pub(crate) fn create_category(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.ask_name("New category", "Category name", "Press Enter to add the category.", "", window, cx, move |this, name, _, cx| {
      if name.trim().is_empty() {
        return Err("Enter a name for the category.".into());
      }
      this.projects.add_category(name);
      this.persist_projects();
      cx.notify();
      Ok(())
    });
  }

  /// Puts a project in the place and the category of another one: dragging a row in the project list in Settings.
  pub(crate) fn move_project_before(&mut self, dragged: &Path, target: &Path, cx: &mut Context<Self>) {
    if self.projects.move_project_before(dragged, target) {
      self.persist_projects();
      cx.notify();
    }
  }

  /// Stars a project, or takes its star away: favorites are listed first in the project menu.
  pub(crate) fn toggle_favorite(&mut self, path: &Path, cx: &mut Context<Self>) {
    let favorite = self.projects.projects.iter().find(|project| project.path == path).is_some_and(|project| project.favorite);
    self.projects.set_favorite(path, !favorite);
    self.persist_projects();
    cx.notify();
  }

  /// Makes a category from a project, unnamed: the menu's way to start one without a second project.
  pub(crate) fn new_category(&mut self, project: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    self.ask_name("New category", "Category name", "Press Enter to add the category.", "", window, cx, move |this, name, _, cx| {
      let id = this.projects.add_category(name);
      this.projects.set_category(&project, Some(&id));
      this.persist_projects();
      cx.notify();
      Ok(())
    });
  }

  /// Renames a category. An empty name makes it unnamed again.
  pub(crate) fn rename_category(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
    let initial = self.projects.categories.iter().find(|category| category.id == id).map(|category| category.name.clone()).unwrap_or_default();
    self.ask_name("Rename category", "Category name", "Press Enter to rename the category.", &initial, window, cx, move |this, name, _, cx| {
      this.projects.rename_category(&id, name);
      this.persist_projects();
      cx.notify();
      Ok(())
    });
  }

  pub(crate) fn delete_category(&mut self, id: &str, cx: &mut Context<Self>) {
    self.projects.remove_category(id);
    self.persist_projects();
    cx.notify();
  }

  pub(crate) fn move_project(&mut self, path: &Path, category: Option<&str>, cx: &mut Context<Self>) {
    self.projects.set_category(path, category);
    self.persist_projects();
    cx.notify();
  }

  /// The projects and categories the project menu shows.
  pub(crate) fn project_list(&self) -> &ProjectList {
    &self.projects
  }

  /// Moves a project up or down among the projects of its category.
  pub(crate) fn move_project_by(&mut self, path: &Path, delta: isize, cx: &mut Context<Self>) {
    self.projects.move_project_by(path, delta);
    self.persist_projects();
    cx.notify();
  }

  /// Moves a category up or down in the menu.
  pub(crate) fn move_category_by(&mut self, id: &str, delta: isize, cx: &mut Context<Self>) {
    self.projects.move_category_by(id, delta);
    self.persist_projects();
    cx.notify();
  }

  /// Asks before taking a project out of Vekro: its shells stop, its folder stays on disk.
  pub(crate) fn confirm_remove_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let Some(project) = self.projects.projects.iter().find(|project| project.path == path) else { return };
    let (title, workspace) = (SharedString::from(format!("Remove {}?", project.name())), cx.entity());
    window.open_alert_dialog(cx, move |alert, _, _| {
      let (workspace, path) = (workspace.clone(), path.clone());
      alert
        .title(title.clone())
        .description("The project leaves the list and its terminals stop. The folder and its files stay on disk.")
        .width(px(430.))
        .confirm()
        .ok_text("Remove")
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, window, cx| {
          workspace.update(cx, |this, cx| this.remove_project(&path, window, cx));
          true
        })
    });
  }

  /// Takes a project out of the list. If it is the open one, another project opens.
  fn remove_project(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
    if self.projects.active.as_deref() == Some(path) {
      let removed = path.to_path_buf();
      self.switch_project(window, cx, move |projects| projects.remove(&removed));
    } else {
      self.projects.remove(path);
      self.persist_projects();
    }
    self.terminal_tabs.remove(path);
    self.views.remove(path);
    cx.notify();
  }

  /// Opens the dialog for a project's name, icon, color and Git remote.
  pub(crate) fn open_project_settings(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
    let Some(project) = self.projects.projects.iter().find(|project| project.path == path).cloned() else { return };
    let settings = cx.new(|cx| ProjectSettings::new(&project, window, cx));
    self._dialog_subscription = Some(cx.subscribe_in(&settings, window, |this, _, chosen: &AppearanceChosen, window, cx| {
      this.projects.set_appearance(&chosen.path, &chosen.name, chosen.icon.as_deref(), chosen.color.as_deref());
      this.persist_projects();
      if let Some(url) = &chosen.remote {
        match vekro_git::set_remote_url(&chosen.path, url) {
          Ok(()) => {
            if this.projects.active.as_ref() == Some(&chosen.path) {
              this.refresh_project();
            }
            window.push_notification(Notification::info("Git remote updated."), cx);
          }
          Err(message) => {
            window.push_notification(Notification::error(format!("Could not change the Git remote: {message}")), cx);
            return;
          }
        }
      }
      window.close_dialog(cx);
      cx.notify();
    }));
    let title = SharedString::from(format!("{} settings", project.name()));
    window.open_dialog(cx, move |dialog, _, _| dialog.title(title.clone()).width(px(480.)).p_5().child(settings.clone()));
  }

  fn select_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    if self.projects.active.as_ref() != Some(&path) {
      self.switch_project(window, cx, move |projects| projects.active = Some(path));
    }
  }

  /// Changes the open project. Everything the window shows belongs to one project: the folder tree, the Git
  /// summary, the file or diff in the editor and the terminals. What the project being left shows is put away
  /// and comes back when it is opened again; its shells keep running.
  fn switch_project(&mut self, window: &mut Window, cx: &mut Context<Self>, change: impl FnOnce(&mut ProjectList)) {
    let before = self.projects.active.clone();
    if let Some(root) = &before {
      let view = ProjectView {
        expanded: std::mem::take(&mut self.expanded),
        selected_entry: self.selected_entry.take(),
        selected_change: self.selected_change.take(),
      };
      self.views.insert(root.clone(), view);
    }
    change(&mut self.projects);
    if let Err(error) = self.projects.save(&ProjectList::default_path()) {
      eprintln!("Could not save the project list: {error}");
    }
    self.excluded.clear();
    self.history.clear();
    self.compare = None;
    self.compare_open = false;
    self.compare_choices = None;
    self.history_limit = HISTORY_PAGE;
    self.selected_commit = None;
    let after = self.projects.active.clone();
    let view = after.as_ref().and_then(|root| self.views.remove(root)).unwrap_or_default();
    self.expanded = view.expanded;
    self.selected_entry = view.selected_entry;
    self.selected_change = view.selected_change;
    if let (Some(root), Some(file)) = (after.clone(), self.selected_change.clone()) {
      self.change_view.update(cx, |view, cx| view.show(root, file, cx));
    }
    self.editor_panel.update(cx, |editor, cx| editor.switch_project(before, after, cx));
    self.refresh_project();
    if self.git_view == GitView::History {
      self.load_history(cx);
    }
    self.ensure_terminal(window, cx);
    cx.notify();
  }

  /// Reads the repository summary and the folder tree of the active project.
  fn refresh_project(&mut self) {
    match &self.projects.active {
      Some(root) => {
        self.git = vekro_git::summary(root);
        self.rows = explorer::rows(root, &self.expanded);
      }
      None => {
        self.git = None;
        self.rows.clear();
      }
    }
  }

  /// Takes projects whose folder was deleted out of the list, off the interface thread, and says which.
  fn sync_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.syncing || self.projects.projects.is_empty() {
      return;
    }
    self.syncing = true;
    let list = self.projects.clone();
    let work = cx.background_executor().spawn(async move { list.missing_paths() });
    cx.spawn_in(window, async move |this, cx| {
      let missing = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.syncing = false;
        let names: Vec<String> =
          missing.iter().filter_map(|path| this.projects.projects.iter().find(|project| &project.path == path)).map(Project::name).collect();
        if names.is_empty() {
          return;
        }
        for path in &missing {
          this.remove_project(path, window, cx);
        }
        window.push_notification(Notification::info(format!("Removed {}: the folder no longer exists.", names.join(", "))), cx);
      });
    })
    .detach();
  }

  /// Reads the repository and the folder tree off the interface thread and shows them only if they changed.
  fn refresh_in_background(&mut self, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let expanded = self.expanded.clone();
    let work = cx.background_executor().spawn({
      let (root, expanded) = (root.clone(), expanded.clone());
      async move { (vekro_git::summary(&root), explorer::rows(&root, &expanded)) }
    });
    cx.spawn(async move |this, cx| {
      let (git, rows) = work.await;
      let _ = this.update(cx, |this, cx| {
        // The user may have switched projects or opened a folder while this was reading.
        if this.projects.active.as_ref() == Some(&root) && this.expanded == expanded && (this.git != git || this.rows != rows) {
          this.git = git;
          this.rows = rows;
          cx.notify();
        }
        if this.sidebar_tab == SidebarTab::Git && this.git_view == GitView::History {
          this.load_history(cx);
        }
        this.sync_menus(false, cx);
        // While the project menu is open, what each project holds is kept up to date.
        if this.projects_menu_open {
          this.load_project_status(cx);
        }
        // The file shown in the Changes view was edited, saved or committed elsewhere: show what it is now.
        if let Some(file) = this.selected_change.clone() {
          if this.git.as_ref().is_some_and(|git| git.changes.iter().any(|change| change.path == file)) {
            this.change_view.update(cx, |view, cx| view.reload(cx));
          } else {
            this.selected_change = None;
          }
        }
      });
    })
    .detach();
  }

  /// Asks for the name of a new file or folder inside the selected folder, next to the selected file, or in the project folder.
  fn new_entry(&mut self, is_dir: bool, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let parent = self
      .selected_entry
      .as_deref()
      .map(|entry| if entry.is_dir() { entry } else { entry.parent().unwrap_or(entry) })
      .filter(|folder| folder.starts_with(&root))
      .map_or_else(|| root.clone(), Path::to_path_buf);
    let (kind, placeholder) = if is_dir { ("folder", "Folder name") } else { ("file", "File name") };
    let place = parent.file_name().map_or_else(|| "project".into(), |name| name.to_string_lossy().into_owned());
    self.ask_name(format!("New {kind} in {place}"), placeholder, "Press Enter to create it.", "", window, cx, move |this, name, window, cx| {
      let path = explorer::create(&parent, name, is_dir)?;
      this.entry_created(path, is_dir, window, cx);
      Ok(())
    });
  }

  /// Opens a dialog with one text field. Enter hands the text to `submit`; when that succeeds the dialog closes,
  /// and when it fails the message shows as a notification and the dialog stays.
  fn ask_name(
    &mut self,
    title: impl Into<SharedString>,
    placeholder: &'static str,
    hint: &'static str,
    initial: &str,
    window: &mut Window,
    cx: &mut Context<Self>,
    submit: impl Fn(&mut Self, &str, &mut Window, &mut Context<Self>) -> Result<(), String> + 'static,
  ) {
    let initial = initial.to_owned();
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(initial));
    self._dialog_subscription = Some(cx.subscribe_in(&input, window, move |this, input, event: &InputEvent, window, cx| {
      if let InputEvent::PressEnter { .. } = event {
        let name = input.read(cx).value().to_string();
        match submit(this, &name, window, cx) {
          Ok(()) => window.close_dialog(cx),
          Err(message) => window.push_notification(Notification::error(message), cx),
        }
      }
    }));
    let title = title.into();
    let field = input.clone();
    let muted = cx.theme().muted_foreground;
    window.open_dialog(cx, move |dialog, _, _| {
      dialog
        .title(title.clone())
        .width(px(420.))
        .p_5()
        .child(v_flex().gap_2().child(Input::new(&field)).child(Label::new(hint).text_size(FONT_SIZE_SM).text_color(muted)))
    });
    let focus = input.read(cx).focus_handle(cx);
    window.defer(cx, move |window, cx| focus.focus(window, cx));
  }

  /// Shows a new file or folder in the tree, and opens a new file.
  fn entry_created(&mut self, path: PathBuf, is_dir: bool, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(root) = self.projects.active.clone() {
      for folder in path.ancestors().skip(1).take_while(|folder| *folder != root && folder.starts_with(&root)) {
        self.expanded.insert(folder.to_path_buf());
      }
    }
    self.refresh_project();
    if is_dir {
      cx.notify();
    } else {
      self.open_file(path, window, cx);
    }
  }

  /// Folds every folder of the tree.
  fn collapse_folders(&mut self, cx: &mut Context<Self>) {
    self.expanded.clear();
    self.refresh_project();
    cx.notify();
  }

  fn toggle_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    if !self.expanded.remove(&path) {
      self.expanded.insert(path);
    }
    self.refresh_project();
    cx.notify();
  }

  /// Opens a file from the Explorer in the editor.
  fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let result = self.editor_panel.update(cx, |editor, cx| editor.open_file(path.clone(), window, cx));
    match result {
      Ok(()) => {
        self.selected_entry = Some(path);
        self.selected_change = None;
      }
      Err(message) => window.push_notification(Notification::error(message), cx),
    }
    cx.notify();
  }

  /// Shows the changes of a changed file in the main area, in place of the editor. Choosing another file replaces them.
  fn open_change(&mut self, file: PathBuf, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    self.change_view.update(cx, |view, cx| view.show(root, file.clone(), cx));
    self.selected_change = Some(file);
    self.selected_entry = None;
    cx.notify();
  }

  /// Opens a changed file itself, not its diff, in the editor.
  pub(crate) fn open_change_in_editor(&mut self, file: &Path, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    match vekro_git::workdir_file(&root, file).filter(|path| path.is_file()) {
      Some(path) => self.open_file(path, window, cx),
      None => window.push_notification(Notification::error(format!("{} is not on the disk.", file.display())), cx),
    }
  }

  /// Asks before throwing away the changes of one file: that cannot be undone.
  pub(crate) fn confirm_discard_changes(&mut self, file: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let (workspace, title) = (cx.entity(), SharedString::from(format!("Discard changes to {}?", file.display())));
    window.open_alert_dialog(cx, move |alert, _, _| {
      let (workspace, file) = (workspace.clone(), file.clone());
      alert
        .title(title.clone())
        .description("Every change to this file, staged or not, is lost. A new file is deleted. This cannot be undone.")
        .width(px(430.))
        .confirm()
        .ok_text("Discard")
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, window, cx| {
          workspace.update(cx, |this, cx| this.discard_changes(&file, window, cx));
          true
        })
    });
  }

  fn discard_changes(&mut self, file: &Path, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    match vekro_git::discard_changes(&root, file) {
      Ok(()) => {
        if let Some(path) = vekro_git::workdir_file(&root, file) {
          self.editor_panel.update(cx, |editor, cx| editor.forget(&path, cx));
        }
        if self.selected_change.as_deref() == Some(file) {
          self.selected_change = None;
        }
        self.refresh_project();
        window.push_notification(Notification::info(format!("Discarded the changes to {}.", file.display())), cx);
        cx.notify();
      }
      Err(message) => window.push_notification(Notification::error(message), cx),
    }
  }

  /// Commits the changed files that are ticked, with the summary and description typed below the list.
  fn commit_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (Some(root), Some(git)) = (self.projects.active.clone(), self.git.as_ref()) else { return };
    if self.committing {
      return;
    }
    let files: Vec<PathBuf> = git.changes.iter().filter(|change| !self.excluded.contains(&change.path)).map(|change| change.path.clone()).collect();
    let summary = self.commit_summary.read(cx).value().to_string();
    let description = self.commit_description.read(cx).value().to_string();
    self.committing = true;
    cx.notify();
    let (push, accounts) = {
      let model = self.settings.read(cx);
      (model.opencommit.git_push, model.accounts.clone())
    };
    let remote_url = self.git.as_ref().and_then(|git| git.sync.as_ref()).map(|sync| sync.url.clone()).unwrap_or_default();
    let work = cx.background_executor().spawn({
      let (root, files) = (root.clone(), files.clone());
      async move {
        let id = vekro_git::commit(&root, &files, &summary, &description)?;
        let pushed = push.then(|| vekro_git::push(&root, credentials_for_repository(&accounts, &root).as_ref(), |_| {}));
        Ok::<_, String>((id, pushed))
      }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.committing = false;
        match result {
          Ok((id, pushed)) => {
            this.commit_summary.update(cx, |input, cx| input.set_value("", window, cx));
            this.commit_description.update(cx, |input, cx| input.set_value("", window, cx));
            this.excluded.clear();
            // The diff of a file that was just committed has nothing left to show.
            if let Some(open) = this.selected_change.clone().filter(|open| files.contains(open)) {
              if let Some(path) = vekro_git::workdir_file(&root, &open) {
                this.editor_panel.update(cx, |editor, cx| editor.forget(&path, cx));
              }
              this.selected_change = None;
            }
            this.refresh_project();
            match pushed {
              None => window.push_notification(Notification::info(format!("Committed {id}.")), cx),
              Some(Ok(_)) => window.push_notification(Notification::info(format!("Committed {id} and pushed.")), cx),
              Some(Err(message)) => {
                if let Some(kind) = refusal_kind(&message) {
                  this.flag_account_issue(&remote_url, kind, cx);
                }
                window.push_notification(Notification::error(format!("Committed {id}, but the push failed: {message}")), cx)
              }
            }
          }
          Err(message) => window.push_notification(Notification::error(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Tells the user once that a newer Vekro is available, in the window they are using.
  fn announce_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if !window.is_window_active() {
      return;
    }
    let announce = {
      let state = &self.settings.read(cx).updates;
      match &state.update {
        AppUpdate::Available(release) if state.announced.as_deref() != Some(release.tag.as_str()) => {
          Some((release.tag.clone(), release.version.clone()))
        }
        _ => None,
      }
    };
    if let Some((tag, version)) = announce {
      self.settings.update(cx, |model, _| model.updates.announced = Some(tag));
      window.push_notification(Notification::info(format!("Vekro {version} is available. Download it in Settings → Updates.")), cx);
    }
  }

  /// Opens the dialog that connects a GitHub, GitLab or Azure DevOps account.
  pub(crate) fn open_connect_account(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let form = cx.new(|cx| ConnectAccount::new(window, cx));
    self._dialog_subscription = Some(cx.subscribe_in(&form, window, |this, _, connected: &AccountConnected, window, cx| {
      let (mut account, warnings, unchecked) = (connected.0.clone(), connected.1.clone(), connected.2.clone());
      // A permission the token certainly lacks stays on the account until a check finds it fixed.
      account.issue = (!warnings.is_empty()).then(|| warnings.join(" "));
      let described = describe(&account);
      this.settings.update(cx, |model, cx| {
        model.accounts.add(account);
        if let Err(error) = model.accounts.save(&accounts_path()) {
          eprintln!("Could not save the accounts: {error}");
        }
        cx.notify();
      });
      window.close_dialog(cx);
      if !warnings.is_empty() {
        window.push_notification(Notification::warning(format!("Connected {described}, but: {}", warnings.join(" "))), cx);
      } else if let Some(note) = unchecked {
        window.push_notification(Notification::info(format!("Connected {described}. {note}")), cx);
      } else {
        window.push_notification(Notification::info(format!("Connected {described}.")), cx);
      }
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Connect an account").width(px(580.)).p_5().child(form.clone()));
  }

  /// Disconnects an account: it leaves the list and its token leaves the Keychain.
  fn remove_account(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
    let described = self.settings.read(cx).accounts.items.iter().find(|account| account.key() == key).map(describe);
    let work = cx.background_executor().spawn({
      let key = key.clone();
      async move { vekro_hosting::TokenStore::delete(&vekro_hosting::KeychainStore, &key) }
    });
    self.settings.update(cx, |model, cx| {
      model.accounts.items.retain(|account| account.key() != key);
      if let Err(error) = model.accounts.save(&accounts_path()) {
        eprintln!("Could not save the accounts: {error}");
      }
      cx.notify();
    });
    cx.spawn(async move |_, _| work.await).detach();
    if let Some(described) = described {
      window.push_notification(Notification::info(format!("Disconnected {described}.")), cx);
    }
  }

  /// Opens the dialog for the commit message generator: the OpenCommit settings and the commit instructions.
  pub(crate) fn open_commit_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (config, instructions) = {
      let model = self.settings.read(cx);
      (model.opencommit.clone(), model.preferences.commit_instructions.clone())
    };
    let form = cx.new(|cx| CommitSettings::new(config, instructions, window, cx));
    self._dialog_subscription = Some(cx.subscribe_in(&form, window, |this, _, saved: &CommitSettingsSaved, window, cx| {
      if let Err(error) = saved.config.save(&vekro_opencommit::config_path()) {
        window.push_notification(Notification::error(format!("Could not save the OpenCommit settings: {error}")), cx);
        return;
      }
      let (config, instructions) = (saved.config.clone(), saved.instructions.clone());
      this.settings.update(cx, |model, cx| {
        model.opencommit = config;
        model.preferences.commit_instructions = instructions;
        cx.notify();
      });
      window.close_dialog(cx);
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.title("OpenCommit settings").width(px(600.)).p_5().child(form.clone()));
  }

  /// Writes the summary and description of the commit from the ticked files' changes, with OpenCommit.
  fn generate_commit_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (Some(root), Some(git)) = (self.projects.active.clone(), self.git.as_ref()) else { return };
    if self.generating {
      return;
    }
    let files: Vec<PathBuf> = git.changes.iter().filter(|change| !self.excluded.contains(&change.path)).map(|change| change.path.clone()).collect();
    if files.is_empty() {
      window.push_notification(Notification::error("Choose at least one changed file to describe."), cx);
      return;
    }
    let (config, instructions) = {
      let model = self.settings.read(cx);
      (model.opencommit.clone(), model.preferences.commit_instructions.clone().unwrap_or_else(|| DEFAULT_INSTRUCTIONS.to_string()))
    };
    if config.api_key.is_empty() && config.api_url.is_empty() {
      window.push_notification(Notification::error("Add your API key to generate commit messages."), cx);
      self.open_commit_settings(window, cx);
      return;
    }
    self.generating = true;
    cx.notify();
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move { vekro_opencommit::generate(&config, &instructions, &vekro_git::diff_files(&root, &files)?) }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.generating = false;
        // The user may have moved to another project while the message was being written.
        if this.projects.active.as_ref() == Some(&root) {
          match result {
            Ok(message) => {
              this.commit_summary.update(cx, |input, cx| input.set_value(message.summary, window, cx));
              this.commit_description.update(cx, |input, cx| input.set_value(message.description, window, cx));
            }
            Err(message) => window.push_notification(Notification::error(message), cx),
          }
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Does what the toolbar's button says: pulls when the remote is ahead, pushes (or publishes) when the branch is, else fetches.
  /// The button shows what git reports while it works.
  fn sync_remote(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (Some(root), Some(sync)) = (self.projects.active.clone(), self.git.as_ref().and_then(|git| git.sync.clone())) else { return };
    if self.remote_work.is_some() {
      return;
    }
    let remote = sync.remote.clone();
    let remote_url = sync.url.clone();
    let (title, done) = if sync.behind > 0 {
      (format!("Pulling from {remote}"), "Pulled")
    } else if !sync.upstream {
      ("Publishing branch".to_string(), "Published")
    } else if sync.ahead > 0 {
      (format!("Pushing to {remote}"), "Pushed")
    } else {
      (format!("Fetching {remote}"), "Fetched")
    };
    let progress = Arc::new(Mutex::new(String::from("Connecting…")));
    self.remote_work = Some(RemoteWork { title: title.into(), progress: progress.clone() });
    cx.notify();
    let accounts = self.settings.read(cx).accounts.clone();
    let work = cx.background_executor().spawn({
      let root = root.clone();
      let report = move |line: &str| *progress.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = line.to_string();
      async move {
        let credentials = credentials_for_repository(&accounts, &root);
        if sync.behind > 0 {
          vekro_git::pull(&root, credentials.as_ref(), report)
        } else if sync.ahead > 0 || !sync.upstream {
          vekro_git::push(&root, credentials.as_ref(), report)
        } else {
          vekro_git::fetch(&root, credentials.as_ref(), report)
        }
      }
    });
    // Redraw a few times a second while git reports, so the progress text moves.
    let ticker = cx.spawn(async move |this, cx| loop {
      cx.background_executor().timer(PROGRESS_INTERVAL).await;
      if this
        .update(cx, |this, cx| {
          if this.remote_work.is_some() {
            cx.notify();
          }
          this.remote_work.is_some()
        })
        .unwrap_or(false)
      {
        continue;
      }
      break;
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.remote_work = None;
        this.refresh_project();
        match result {
          Ok(_) => window.push_notification(Notification::info(format!("{done} {remote}.")), cx),
          Err(message) => {
            if let Some(kind) = refusal_kind(&message) {
              this.flag_account_issue(&remote_url, kind, cx);
            }
            window.push_notification(Notification::error(message), cx)
          }
        }
        cx.notify();
      });
      drop(ticker);
    })
    .detach();
  }

  /// The address of the repository's remote and the repository it names, when a connected account serves its host: pull requests
  /// can be used.
  fn hosted_remote(&self, cx: &App) -> Option<(String, RepoRef)> {
    let url = self.git.as_ref()?.sync.as_ref()?.url.clone();
    let accounts = &self.settings.read(cx).accounts;
    accounts.items.iter().find_map(|account| RepoRef::for_account(account, &url)).map(|repository| (url, repository))
  }

  /// Switches the Git tab between the files to commit and the commits already made.
  fn set_git_view(&mut self, view: GitView, cx: &mut Context<Self>) {
    if self.git_view == view {
      return;
    }
    self.git_view = view;
    if view == GitView::History {
      self.load_history(cx);
    }
    cx.notify();
  }

  /// Reads the commits of the open project off the interface thread; the first one is shown when none is chosen.
  fn load_history(&mut self, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    self.history_epoch += 1;
    let epoch = self.history_epoch;
    let limit = self.history_limit;
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move { vekro_git::history(&root, limit) }
    });
    cx.spawn(async move |this, cx| {
      let found = work.await.unwrap_or_default();
      let _ = this.update(cx, |this, cx| {
        if this.history_epoch != epoch || this.projects.active.as_ref() != Some(&root) {
          return;
        }
        let known = this.selected_commit.as_ref().is_some_and(|id| found.iter().any(|commit| &commit.id == id));
        let first = found.first().map(|commit| commit.id.clone());
        if this.history != found {
          this.history = found;
          cx.notify();
        }
        // While a comparison is shown, the commit chosen in it stays.
        if !known && this.compare.is_none() {
          match first {
            Some(id) => this.select_commit(id, cx),
            None => this.selected_commit = None,
          }
        }
      });
    })
    .detach();
  }

  /// Takes the last commit back, keeping its changes: they return to the Changes view with the commit's message in the commit box.
  fn undo_commit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let work = cx.background_executor().spawn(async move { vekro_git::undo_commit(&root, &id) });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        match result {
          Ok((summary, description)) => {
            this.commit_summary.update(cx, |input, cx| input.set_value(summary, window, cx));
            this.commit_description.update(cx, |input, cx| input.set_value(description, window, cx));
            this.refresh_project();
            this.selected_commit = None;
            this.git_view = GitView::Changes;
            window
              .push_notification(Notification::info("Undid the commit: its changes are back in Changes, with its message in the commit box."), cx);
          }
          Err(message) => window.push_notification(Notification::error(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Asks for a name and tags a commit with it.
  fn create_tag(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
    let short: String = id.chars().take(7).collect();
    self.ask_name(
      format!("Create a tag at {short}"),
      "Tag name, such as v1.0.0",
      "Press Enter to tag the commit.",
      "",
      window,
      cx,
      move |this, name, window, cx| {
        let root = this.projects.active.clone().ok_or("Open a project first.")?;
        vekro_git::create_tag(&root, &id, name)?;
        this.load_history(cx);
        window.push_notification(Notification::info(format!("Tagged {short} as {}. The tag is local until you push it.", name.trim())), cx);
        Ok(())
      },
    );
  }

  /// Loads the next page of commits when the History list has been scrolled to its end. Called after the list was scrolled, once its
  /// position has settled, and by the button at the end of the list.
  fn load_more_history(&mut self, only_at_end: bool, cx: &mut Context<Self>) {
    let more_may_exist = self.history.len() >= self.history_limit;
    let at_end = {
      let (offset, max) = (self.history_scroll.offset(), self.history_scroll.max_offset());
      let (travelled, total) = (offset.y.abs(), max.y.abs());
      total <= px(0.) || travelled >= total - HISTORY_END_MARGIN
    };
    if self.git_view != GitView::History || !more_may_exist || (only_at_end && !at_end) {
      return;
    }
    self.history_limit += HISTORY_PAGE;
    self.load_history(cx);
  }

  /// Opens the list of branches under the compare field, reading them off the interface thread.
  fn open_compare_list(&mut self, _: &mut Window, cx: &mut Context<Self>) {
    if self.compare_open || self.compare.is_some() {
      return;
    }
    let Some(root) = self.projects.active.clone() else { return };
    self.compare_open = true;
    self.compare_choices = None;
    self.compare_epoch += 1;
    let epoch = self.compare_epoch;
    let work = cx.background_executor().spawn(async move { vekro_git::compare_choices(&root) });
    cx.spawn(async move |this, cx| {
      let found = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.compare_epoch == epoch && this.compare_open {
          this.compare_choices = Some(found.unwrap_or_default());
          cx.notify();
        }
      });
    })
    .detach();
    cx.notify();
  }

  /// Closes the list of branches and empties the field.
  fn close_compare_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.compare_open = false;
    self.compare_epoch += 1;
    self.compare_filter.update(cx, |input, cx| input.set_value("", window, cx));
    window.blur(cx);
    cx.notify();
  }

  /// The branches the field's text matches, in the list's three groups.
  fn matching_compare_branches(&self, cx: &App) -> (Option<BranchRef>, Vec<BranchRef>, Vec<BranchRef>) {
    let query = self.compare_filter.read(cx).value().trim().to_lowercase();
    let fits = |branch: &BranchRef| query.is_empty() || branch.name.to_lowercase().contains(&query);
    match &self.compare_choices {
      Some(choices) => (
        choices.default.clone().filter(fits),
        choices.recent.iter().filter(|branch| fits(branch)).cloned().collect(),
        choices.other.iter().filter(|branch| fits(branch)).cloned().collect(),
      ),
      None => (None, Vec::new(), Vec::new()),
    }
  }

  /// Enter in the field: compares with the first branch that matches.
  fn choose_first_compare_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (default, recent, other) = self.matching_compare_branches(cx);
    if let Some(branch) = default.into_iter().chain(recent).chain(other).next() {
      self.choose_compare_branch(branch.name, window, cx);
    }
  }

  /// Compares the History list with a branch: the commits each side has that the other lacks, read off the interface thread.
  fn choose_compare_branch(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    self.close_compare_list(window, cx);
    self.compare = Some(CompareView { branch: name.clone(), side: CompareSide::Behind, found: None });
    let epoch = self.compare_epoch;
    let work = cx.background_executor().spawn({
      let name = name.clone();
      async move { vekro_git::compare(&root, &name) }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        if this.compare_epoch != epoch || this.compare.as_ref().map(|view| view.branch.as_str()) != Some(name.as_str()) {
          return;
        }
        match result {
          Ok(found) => {
            let side = if found.behind.is_empty() && !found.ahead.is_empty() { CompareSide::Ahead } else { CompareSide::Behind };
            let first = match side {
              CompareSide::Behind => found.behind.first(),
              CompareSide::Ahead => found.ahead.first(),
            }
            .map(|commit| commit.id.clone());
            if let Some(view) = this.compare.as_mut() {
              view.side = side;
              view.found = Some(found);
            }
            if let Some(id) = first {
              this.select_commit(id, cx);
            }
          }
          Err(message) => {
            this.compare = None;
            window.push_notification(Notification::error(message), cx);
          }
        }
        cx.notify();
      });
    })
    .detach();
    cx.notify();
  }

  /// Shows the other side of the comparison.
  fn set_compare_side(&mut self, side: CompareSide, cx: &mut Context<Self>) {
    let Some(view) = self.compare.as_mut() else { return };
    view.side = side;
    let first = view.found.as_ref().and_then(|found| match side {
      CompareSide::Behind => found.behind.first(),
      CompareSide::Ahead => found.ahead.first(),
    });
    if let Some(id) = first.map(|commit| commit.id.clone()) {
      self.select_commit(id, cx);
    }
    cx.notify();
  }

  /// Goes back from a comparison to the plain History list.
  fn clear_compare(&mut self, cx: &mut Context<Self>) {
    self.compare = None;
    self.compare_epoch += 1;
    if let Some(id) = self.history.first().map(|commit| commit.id.clone()) {
      self.select_commit(id, cx);
    }
    cx.notify();
  }

  /// Merges the compared branch into the current one, like `git merge`, once the user asks for it. Conflicts stop it and leave
  /// everything as it was.
  fn merge_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (Some(root), Some(branch)) = (self.projects.active.clone(), self.compare.as_ref().map(|view| view.branch.clone())) else { return };
    if self.merging {
      return;
    }
    self.merging = true;
    cx.notify();
    let work = cx.background_executor().spawn({
      let branch = branch.clone();
      async move { vekro_git::merge_branch(&root, &branch) }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.merging = false;
        this.refresh_project();
        match result {
          Ok(()) => {
            this.compare = None;
            this.load_history(cx);
            window.push_notification(Notification::info(format!("Merged {branch}.")), cx);
          }
          Err(message) => window.push_notification(Notification::error(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Shows a commit of the History view in the main area.
  fn select_commit(&mut self, id: String, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    self.selected_commit = Some(id.clone());
    self.commit_view.update(cx, |view, cx| view.show(root, id, cx));
    cx.notify();
  }

  /// Shows a tab of the branch menu, with an empty filter.
  fn set_branch_tab(&mut self, tab: BranchTab, window: &mut Window, cx: &mut Context<Self>) {
    if self.branch_tab == tab {
      return;
    }
    self.branch_tab = tab;
    self.branch_filter.update(cx, |input, cx| input.set_value("", window, cx));
    let focus = self.branch_filter.read(cx).focus_handle(cx);
    window.defer(cx, move |window, cx| focus.focus(window, cx));
    if tab == BranchTab::PullRequests {
      self.load_pull_requests(cx);
    }
    cx.notify();
  }

  /// Reads the open pull requests of the repository off the interface thread.
  fn load_pull_requests(&mut self, cx: &mut Context<Self>) {
    let (Some((url, _)), Some(root)) = (self.hosted_remote(cx), self.projects.active.clone()) else { return };
    let accounts = self.settings.read(cx).accounts.clone();
    self.pull_requests = PullRequests::Loading;
    self.pr_epoch += 1;
    let epoch = self.pr_epoch;
    let work = cx.background_executor().spawn(async move {
      let (connection, repository) = accounts
        .for_remote(&url, &KeychainStore)
        .ok_or("The token for this account is not in the Keychain. Connect the account again in Settings → Accounts.")?;
      vekro_hosting::pull_requests(&connection, &repository)
    });
    cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.pr_epoch == epoch && this.projects.active.as_ref() == Some(&root) {
          this.pull_requests = match result {
            Ok(found) => PullRequests::Loaded(found),
            Err(message) => PullRequests::Failed(message.into()),
          };
          cx.notify();
        }
      });
    })
    .detach();
  }

  /// Opens the dialog that creates a pull request for the current branch.
  fn open_pull_request_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.branch_menu_open = false;
    cx.notify();
    let (Some(root), Some(git), Some((url, _))) = (self.projects.active.clone(), self.git.as_ref(), self.hosted_remote(cx)) else { return };
    let Some(sync) = git.sync.as_ref() else { return };
    let target = PullRequestTarget {
      remote_url: url,
      source: git.branch.clone(),
      unpushed: sync.ahead,
      published: sync.upstream,
      suggested_title: vekro_git::head_summary(&root).unwrap_or_else(|| git.branch.clone()),
    };
    let accounts = self.settings.read(cx).accounts.clone();
    let form = cx.new(|cx| PullRequestForm::new(accounts, target, window, cx));
    self._dialog_subscription = Some(cx.subscribe_in(&form, window, |this, form, event: &PullRequestEvent, window, cx| match event {
      PullRequestEvent::Opened(opened) => {
        window.close_dialog(cx);
        window.push_notification(Notification::info(format!("Opened pull request #{}: {}", opened.number, opened.title)), cx);
        cx.open_url(&opened.url);
        this.load_pull_requests(cx);
      }
      PullRequestEvent::Refused => {
        let url = form.read(cx).remote_url().to_string();
        this.flag_account_issue(&url, IssueKind::PullRequests, cx);
      }
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Create a pull request").width(px(600.)).p_5().child(form.clone()));
  }

  /// Opens or closes the branch menu; opening reads the local branches and clears the filter.
  fn set_branch_menu(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
    self.branch_menu_open = open;
    if open {
      self.branch_tab = BranchTab::Branches;
      if let Some(root) = &self.projects.active {
        self.branches = vekro_git::branches(root).unwrap_or_default();
      }
      self.branch_filter.update(cx, |input, cx| input.set_value("", window, cx));
      let focus = self.branch_filter.read(cx).focus_handle(cx);
      window.defer(cx, move |window, cx| focus.focus(window, cx));
    }
    cx.notify();
  }

  /// Switches the open project's repository to another local branch. Changes the switch would overwrite stop it.
  fn switch_branch(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
    self.branch_menu_open = false;
    cx.notify();
    let Some(root) = self.projects.active.clone() else { return };
    if self.git.as_ref().is_some_and(|git| git.branch == name) {
      return;
    }
    let work = cx.background_executor().spawn({
      let (root, name) = (root.clone(), name.clone());
      async move { vekro_git::switch_branch(&root, &name) }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        match result {
          Ok(()) => {
            this.refresh_project();
            window.push_notification(Notification::info(format!("Switched to {name}.")), cx);
          }
          Err(message) => window.push_notification(Notification::error(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Asks for a name, creates a branch at the current commit and switches to it. Uncommitted changes come along.
  fn new_branch(&mut self, initial: String, window: &mut Window, cx: &mut Context<Self>) {
    self.branch_menu_open = false;
    self.ask_name(
      "New branch",
      "Branch name",
      "Press Enter to create the branch and switch to it.",
      &initial,
      window,
      cx,
      move |this, name, window, cx| {
        let root = this.projects.active.clone().ok_or("Open a project first.")?;
        vekro_git::create_branch(&root, name)?;
        this.refresh_project();
        window.push_notification(Notification::info(format!("Created and switched to {}.", name.trim())), cx);
        Ok(())
      },
    );
  }

  /// Opens or closes the repository menu of the title bar; opening clears the filter and focuses it.
  fn set_projects_menu(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
    self.projects_menu_open = open;
    if open {
      self.load_project_status(cx);
      self.project_filter.update(cx, |input, cx| input.set_value("", window, cx));
      let focus = self.project_filter.read(cx).focus_handle(cx);
      window.defer(cx, move |window, cx| focus.focus(window, cx));
    }
    cx.notify();
  }

  /// Reads what every project holds (changed files, commits to push) off the interface thread, for the project menu.
  fn load_project_status(&mut self, cx: &mut Context<Self>) {
    let paths: Vec<PathBuf> = self.projects.projects.iter().map(|project| project.path.clone()).collect();
    let work = cx.background_executor().spawn(async move {
      paths
        .into_iter()
        .filter_map(|path| vekro_git::quick_status(&path).map(|(changed, push)| (path, ProjectStatus { changed, push })))
        .collect::<HashMap<_, _>>()
    });
    cx.spawn(async move |this, cx| {
      let found = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.project_status != found {
          this.project_status = found;
          cx.notify();
        }
      });
    })
    .detach();
  }

  /// ⌘W: closes the open tab of the editor, then the others from the last to the first, and keeps the focus in the window so
  /// the shortcut keeps working. It never closes the window, and does nothing while a terminal has the keyboard.
  pub fn close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
    // In a terminal, ⌘W closes the pane that has the keyboard.
    if self.terminal_focused(window, cx) {
      if let Some(terminal) = self.focused_terminal(window, cx) {
        self.close_pane(terminal, window, cx);
      }
      return;
    }
    if self.editor_panel.update(cx, |editor, cx| editor.close_by_shortcut(cx)) {
      // The closed tab held the focus; without a focused element in the window the next ⌘W would reach nothing.
      self.focus_handle.focus(window, cx);
    }
  }

  /// Saves the file open in the editor.
  pub fn save_file(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
    match self.editor_panel.update(cx, |editor, cx| editor.save(cx)) {
      Ok(_) => cx.notify(),
      Err(error) => window.push_notification(Notification::error(format!("Could not save: {error}")), cx),
    }
    // The file's own change shows up in the Git list without waiting for the next read.
    self.refresh_in_background(cx);
  }

  /// The thin bar at the bottom of the window: the branch and changes on the left, the open file on the right.
  fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let file = self.editor_panel.read(cx).status(cx);
    let mut bar = StatusBar::new().bg(palette.surface_alt).border_color(palette.border).text_color(palette.text_secondary);
    if let Some(git) = &self.git {
      bar = bar
        .left(
          Button::new("status-branch")
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/git-branch.svg"))
            .label(git.branch.clone())
            .tooltip("Current branch"),
        )
        .left(if git.changed() == 0 { "No changes".to_string() } else { format!("{} changed", git.changed()) });
    }
    if let Some(project) = self.projects.active.as_ref().map(|path| display_path(path)) {
      bar = bar.child(project);
    }
    for item in file {
      bar = bar.right(item);
    }
    bar
  }

  /// The branch name with a menu: the local branches (filtered by typing), and a way to start a new one.
  fn render_branch_picker(&self, current: &str, cx: &mut Context<Self>) -> impl IntoElement {
    let workspace = cx.entity();
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let current = SharedString::from(current.to_string());
    let trigger = Button::new("branch-trigger")
      .custom(ButtonCustomVariant::new(cx).color(palette.surface_alt).foreground(palette.text).hover(palette.hover).active(palette.selected))
      .border_0()
      .h_12()
      .w_48()
      .rounded_none()
      .child(
        h_flex()
          .w_full()
          .gap_2()
          .items_center()
          .child(Icon::empty().path("icons/git-branch.svg").size_5().text_color(theme.muted_foreground))
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .items_start()
              .child(Label::new("Current branch").text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
              .child(
                Label::new(current.clone()).font_weight(FontWeight::SEMIBOLD).max_w_full().overflow_hidden().whitespace_nowrap().text_ellipsis(),
              ),
          )
          .child(Icon::new(IconName::ChevronDown).size_4().text_color(theme.muted_foreground)),
      )
      .tooltip("Switch branch");
    Popover::new("branch-menu")
      .anchor(Anchor::TopLeft)
      .open(self.branch_menu_open)
      .on_open_change({
        let workspace = workspace.clone();
        move |open, window, cx| workspace.update(cx, |this, cx| this.set_branch_menu(*open, window, cx))
      })
      .trigger(trigger)
      .content(move |_, _, cx| {
        let theme = cx.theme().clone();
        let palette = colors(cx);
        let (filter, query, branches, tab, hosted, state) = {
          let this = workspace.read(cx);
          (
            this.branch_filter.clone(),
            this.branch_filter.read(cx).value().to_string(),
            this.branches.clone(),
            this.branch_tab,
            this.hosted_remote(cx).map(|(_, repository)| repository.full_name()),
            this.pull_requests.clone(),
          )
        };
        let lowered = query.trim().to_lowercase();
        let tab = if hosted.is_some() { tab } else { BranchTab::Branches };

        // The tabs: Branches, and Pull Requests when a connected account serves the repository.
        let count = match &state {
          PullRequests::Loaded(found) if !found.is_empty() => format!(" ({})", found.len()),
          _ => String::new(),
        };
        let tabs = hosted.as_ref().map(|_| {
          let workspace = workspace.clone();
          TabBar::new("branch-menu-tabs")
            .underline()
            .selected_index(usize::from(tab == BranchTab::PullRequests))
            .on_click(move |index: &usize, window, cx| {
              let tab = if *index == 1 { BranchTab::PullRequests } else { BranchTab::Branches };
              workspace.update(cx, |this, cx| this.set_branch_tab(tab, window, cx));
            })
            .children([Tab::new().label("Branches"), Tab::new().label(format!("Pull Requests{count}"))])
        });

        let body = match tab {
          BranchTab::Branches => {
            let rows: Vec<AnyElement> = branches
              .into_iter()
              .filter(|name| lowered.is_empty() || name.to_lowercase().contains(&lowered))
              .map(|name| {
                let selected = name == current.as_ref();
                let workspace = workspace.clone();
                h_flex()
                  .id(SharedString::from(format!("branch-{name}")))
                  .gap_2()
                  .px_2()
                  .py_1()
                  .rounded_md()
                  .items_center()
                  .cursor_pointer()
                  .hover(|row| row.bg(palette.hover))
                  .child(div().size_4().flex_none().when(selected, |mark| mark.child(Icon::new(IconName::Check).size_4())))
                  .child(Label::new(name.clone()).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
                  .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.switch_branch(name.clone(), window, cx)))
                  .into_any_element()
              })
              .collect();
            let new_branch = {
              let (workspace, suggestion) = (workspace.clone(), query.trim().to_string());
              Button::new("branch-create")
                .outline()
                .label("New Branch")
                .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.new_branch(suggestion.clone(), window, cx)))
            };
            v_flex()
              .gap_2()
              .child(h_flex().gap_2().items_center().child(div().flex_1().child(Input::new(&filter))).child(new_branch))
              .child(
                v_flex()
                  .id("branch-list")
                  .max_h(MENU_LIST_MAX_HEIGHT)
                  .overflow_y_scrollbar()
                  .gap_0p5()
                  .when(rows.is_empty(), |list| list.child(Label::new("No matching branches").px_2().py_1().text_color(theme.muted_foreground)))
                  .children(rows),
              )
              .into_any_element()
          }
          BranchTab::PullRequests => {
            let refresh = {
              let workspace = workspace.clone();
              Button::new("pull-requests-refresh")
                .outline()
                .icon(Icon::empty().path("icons/refresh-cw.svg"))
                .tooltip("Refresh")
                .on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.load_pull_requests(cx)))
            };
            let list = match state {
              PullRequests::Idle | PullRequests::Loading => h_flex()
                .gap_2()
                .p_3()
                .items_center()
                .justify_center()
                .child(Spinner::new().small())
                .child(Label::new("Loading pull requests…").text_color(theme.muted_foreground))
                .into_any_element(),
              PullRequests::Failed(message) => Label::new(message).p_2().text_color(theme.danger).into_any_element(),
              PullRequests::Loaded(found) => {
                let shown: Vec<_> = found
                  .iter()
                  .filter(|request| {
                    lowered.is_empty()
                      || request.title.to_lowercase().contains(&lowered)
                      || request.author.to_lowercase().contains(&lowered)
                      || format!("#{}", request.number).contains(&lowered)
                  })
                  .cloned()
                  .collect();
                if found.is_empty() {
                  // GitHub Desktop's empty state: nothing to do here, and a way to get going.
                  let workspace = workspace.clone();
                  v_flex()
                    .items_center()
                    .gap_1()
                    .py_4()
                    .child(Icon::empty().path("icons/circle-check.svg").size_8().text_color(palette.accent))
                    .child(Label::new("You're all set!").font_weight(FontWeight::SEMIBOLD))
                    .child(Label::new(format!("No open pull requests in {}", hosted.clone().unwrap_or_default())).text_color(theme.muted_foreground))
                    .child(
                      Label::new("Would you like to create a new branch and get going on your next project?")
                        .text_size(FONT_SIZE_SM)
                        .text_color(theme.muted_foreground)
                        .text_center()
                        .px_3(),
                    )
                    .child(
                      Button::new("pull-requests-new-branch")
                        .ghost()
                        .small()
                        .label("Create a new branch")
                        .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.new_branch(String::new(), window, cx))),
                    )
                    .into_any_element()
                } else if shown.is_empty() {
                  Label::new("No matching pull requests").px_2().py_1().text_color(theme.muted_foreground).into_any_element()
                } else {
                  v_flex()
                    .id("pull-request-list")
                    .max_h(MENU_LIST_MAX_HEIGHT)
                    .overflow_y_scrollbar()
                    .gap_0p5()
                    .children(shown.into_iter().map(|request| {
                      let url = request.url.clone();
                      v_flex()
                        .id(SharedString::from(format!("pull-request-{}", request.url)))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|row| row.bg(palette.hover))
                        .child(
                          Label::new(if request.draft { format!("{} (draft)", request.title) } else { request.title.clone() })
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis(),
                        )
                        .child(
                          Label::new(format!("#{} · {} · {} → {}", request.number, request.author, request.source, request.target))
                            .text_size(FONT_SIZE_SM)
                            .text_color(theme.muted_foreground)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis(),
                        )
                        .on_click(move |_, _, cx| cx.open_url(&url))
                    }))
                    .into_any_element()
                }
              }
            };
            let create = {
              let workspace = workspace.clone();
              Button::new("pull-request-create")
                .outline()
                .w_full()
                .icon(Icon::empty().path("icons/git-pull-request-create.svg"))
                .label("Create pull request…")
                .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.open_pull_request_form(window, cx)))
            };
            v_flex()
              .gap_2()
              .child(h_flex().gap_2().items_center().child(div().flex_1().child(Input::new(&filter))).child(refresh))
              .child(list)
              .child(div().h_px().bg(palette.border))
              .child(create)
              .into_any_element()
          }
        };
        v_flex().w(MENU_WIDTH).gap_2().children(tabs).child(body)
      })
  }

  /// The title bar's first tab, fixed at the left: the name of the open project with a menu of every project, grouped by category and
  /// filtered by typing, and a way to add one.
  fn render_repository_switcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let workspace = cx.entity();
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let name = self.projects.active.as_ref().and_then(|path| self.projects.projects.iter().find(|project| &project.path == path)).map(Project::name);
    let trigger = Button::new("repository-trigger")
      .custom(ButtonCustomVariant::new(cx).color(palette.rail).foreground(palette.text).hover(palette.rail_item_hover).active(palette.selected))
      .border_0()
      .h_8()
      .max_w_56()
      .rounded_md()
      .child(
        h_flex()
          .gap_2()
          .items_center()
          .child(Icon::empty().path("icons/folder.svg").size_5())
          .child(
            Label::new(name.unwrap_or_else(|| "No project".to_string()))
              .min_w_0()
              .font_weight(FontWeight::SEMIBOLD)
              .overflow_hidden()
              .whitespace_nowrap()
              .text_ellipsis(),
          )
          .child(
            Icon::new(if self.projects_menu_open { IconName::ChevronUp } else { IconName::ChevronDown }).size_4().text_color(theme.muted_foreground),
          ),
      );
    Popover::new("repository-menu")
      .anchor(Anchor::TopLeft)
      .open(self.projects_menu_open)
      .on_open_change({
        let workspace = workspace.clone();
        move |open, window, cx| workspace.update(cx, |this, cx| this.set_projects_menu(*open, window, cx))
      })
      .trigger(trigger)
      .content(move |_, _, cx| {
        let theme = cx.theme().clone();
        let (filter, query, list, statuses) = {
          let this = workspace.read(cx);
          (this.project_filter.clone(), this.project_filter.read(cx).value().to_lowercase(), this.projects.clone(), this.project_status.clone())
        };
        let active = list.active.clone();
        let matches = |project: &&Project| query.is_empty() || project.name().to_lowercase().contains(&query);
        // The starred projects come first, and are not listed again under their category.
        let mut groups: Vec<(SharedString, Vec<&Project>)> = vec![(SharedString::from("Favorites"), list.favorites().filter(matches).collect())];
        groups.extend(list.categories.iter().enumerate().map(|(index, category)| {
          (
            SharedString::from(category_label(index, category)),
            list.projects_in(Some(&category.id)).filter(|project| !project.favorite).filter(matches).collect(),
          )
        }));
        groups.push((SharedString::from("No category"), list.projects_in(None).filter(|project| !project.favorite).filter(matches).collect()));
        groups.retain(|(_, projects)| !projects.is_empty());
        let named = groups.len() > 1 || !list.categories.is_empty() || list.favorites().next().is_some();
        let mut rows: Vec<AnyElement> = Vec::new();
        for (label, projects) in groups {
          if named {
            rows.push(
              Label::new(label)
                .px_2()
                .pt_2()
                .text_size(FONT_SIZE_SM)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .into_any_element(),
            );
          }
          rows.extend(
            projects
              .into_iter()
              .map(|project| project_list_row(&workspace, project, active.as_ref() == Some(&project.path), statuses.get(&project.path), cx)),
          );
        }
        let add = {
          let workspace = workspace.clone();
          Button::new("repository-add").outline().label("Add").icon(IconName::Plus).on_click(move |_, window, cx| {
            workspace.update(cx, |this, cx| {
              this.projects_menu_open = false;
              cx.notify();
            });
            window.dispatch_action(Box::new(AddProject), cx);
          })
        };
        // Sorting, grouping and renaming projects is done in Settings → Project list.
        let sort = {
          let workspace = workspace.clone();
          Button::new("repository-sort")
            .outline()
            .icon(Icon::empty().path("icons/arrow-up-down.svg"))
            .tooltip("Sort and group the projects (Settings → Project list)")
            .on_click(move |_, window, cx| {
              workspace.update(cx, |this, cx| {
                this.projects_menu_open = false;
                this.open_settings_page("Project list", window, cx);
              })
            })
        };
        v_flex()
          .w(PROJECT_MENU_WIDTH)
          .gap_1()
          .child(h_flex().gap_2().items_center().child(div().flex_1().child(Input::new(&filter))).child(add).child(sort))
          .child(
            v_flex()
              .id("repository-list")
              .max_h(MENU_LIST_MAX_HEIGHT)
              .overflow_y_scrollbar()
              .gap_0p5()
              .when(rows.is_empty(), |list| list.child(Label::new("No matching projects").px_2().py_1().text_color(theme.muted_foreground)))
              .children(rows),
          )
      })
  }

  /// The bar above the editor in the Git tab: the branch with its menu, and the one button that keeps the branch in step with
  /// its remote: publish, push, pull or fetch, whichever the branch needs.
  fn render_git_toolbar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    let git = self.git.as_ref()?;
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let picker = self.render_branch_picker(&git.branch, cx);
    let sync = git.sync.as_ref().map(|sync| {
      let (icon, title, count) = if !sync.upstream {
        ("icons/upload.svg", "Publish branch".to_string(), 0)
      } else if sync.behind > 0 {
        ("icons/arrow-down.svg", format!("Pull {}", sync.remote), sync.behind)
      } else if sync.ahead > 0 {
        ("icons/arrow-up.svg", format!("Push {}", sync.remote), sync.ahead)
      } else {
        ("icons/refresh-cw.svg", format!("Fetch {}", sync.remote), 0)
      };
      let detail = if let Some(work) = &self.remote_work {
        work.progress.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
      } else if !sync.upstream {
        format!("Send it to {}", sync.remote)
      } else {
        match sync.last_fetched {
          Some(time) => format!("Last fetched {}", time_ago(time)),
          None => "Never fetched".to_string(),
        }
      };
      Button::new("sync-remote")
        .custom(ButtonCustomVariant::new(cx).color(palette.surface_alt).foreground(palette.text).hover(palette.hover).active(palette.selected))
        .border_0()
        .h_12()
        .w_56()
        .rounded_none()
        .disabled(self.remote_work.is_some())
        .child(
          h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(match &self.remote_work {
              Some(_) => Spinner::new().small().into_any_element(),
              None => Icon::empty().path(icon).size_5().text_color(theme.muted_foreground).into_any_element(),
            })
            .child(
              v_flex()
                .flex_1()
                .min_w_0()
                .items_start()
                .child(Label::new(self.remote_work.as_ref().map_or(title.into(), |work| work.title.clone())).font_weight(FontWeight::SEMIBOLD))
                .child(
                  Label::new(detail)
                    .text_size(FONT_SIZE_SM)
                    .text_color(theme.muted_foreground)
                    .max_w_full()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis(),
                ),
            )
            // How many commits are waiting to be pushed (or pulled): a filled blue circle, easy to see.
            .when(count > 0, |row| {
              row.child(
                div()
                  .min_w_5()
                  .h_5()
                  .px_1p5()
                  .flex()
                  .flex_none()
                  .items_center()
                  .justify_center()
                  .rounded_full()
                  .bg(theme.primary)
                  .text_color(palette.on_accent)
                  .text_size(FONT_SIZE_SM)
                  .font_weight(FontWeight::SEMIBOLD)
                  .child(count.to_string()),
              )
            }),
        )
        .on_click(cx.listener(|this, _, window, cx| this.sync_remote(window, cx)))
    });
    Some(
      h_flex()
        .flex_none()
        .h_12()
        .bg(palette.surface_alt)
        .border_b_1()
        .border_color(palette.border)
        .child(div().h_12().border_r_1().border_color(palette.border).child(picker))
        .children(sync.map(|button| div().h_12().border_r_1().border_color(palette.border).child(button))),
    )
  }

  fn render_git(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let git = self.git.as_ref()?;
    let changed = match git.changed() {
      0 => "No changes".to_string(),
      1 => "1 changed file".to_string(),
      count => format!("{count} changed files"),
    };
    let workspace = cx.entity();
    let changes_list = v_flex().id("git-changes").flex_1().min_h_0().overflow_y_scrollbar().children(git.changes.iter().map(|change| {
      let file = change.path.clone();
      let selected = self.selected_change.as_ref() == Some(&change.path);
      let included = !self.excluded.contains(&change.path);
      let name = change.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
      let folder = change.path.parent().map(|parent| parent.display().to_string()).filter(|parent| !parent.is_empty());
      div().w_full().pr_3().child(
        h_flex()
          .id(SharedString::from(format!("change-row-{}", change.path.display())))
          .gap_1p5()
          .pl_2()
          .pr_2()
          .py_1()
          .rounded_md()
          .items_center()
          .when(selected, |row| row.bg(palette.selected))
          .hover(|row| row.bg(palette.hover))
          .child(Checkbox::new(SharedString::from(format!("include-{}", change.path.display()))).checked(included).on_click(cx.listener({
            let file = file.clone();
            move |this, checked: &bool, _, cx| {
              if *checked {
                this.excluded.remove(&file);
              } else {
                this.excluded.insert(file.clone());
              }
              cx.notify();
            }
          })))
          .child(
            h_flex()
              .id(SharedString::from(format!("change-{}", change.path.display())))
              .flex_1()
              .min_w_0()
              .gap_1p5()
              .items_center()
              .cursor_pointer()
              .child(Label::new(change.kind.mark()).w_4().flex_none().text_color(kind_color(change.kind, &palette)))
              .child(Label::new(name).min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
              .when_some(folder, |row, folder| {
                row.child(
                  Label::new(folder)
                    .text_size(FONT_SIZE_SM)
                    .text_color(theme.muted_foreground)
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis(),
                )
              })
              .on_click(cx.listener({
                let file = file.clone();
                move |this, _, _, cx| this.open_change(file.clone(), cx)
              })),
          )
          .context_menu({
            let (workspace, deleted) = (workspace.clone(), change.kind == vekro_git::ChangeKind::Deleted);
            move |menu, _, _| {
              let (open_workspace, open_file) = (workspace.clone(), file.clone());
              let (discard_workspace, discard_file) = (workspace.clone(), file.clone());
              menu
                .item(
                  PopupMenuItem::new("Open in Editor")
                    .disabled(deleted)
                    .on_click(move |_, window, cx| open_workspace.update(cx, |this, cx| this.open_change_in_editor(&open_file, window, cx))),
                )
                .item(PopupMenuItem::new("Discard changes").on_click(move |_, window, cx| {
                  discard_workspace.update(cx, |this, cx| this.confirm_discard_changes(discard_file.clone(), window, cx))
                }))
            }
          }),
      )
    }));
    let chosen = git.changes.iter().filter(|change| !self.excluded.contains(&change.path)).count();
    let summary_filled = !self.commit_summary.read(cx).value().trim().is_empty();
    let label = match chosen {
      1 => format!("Commit 1 file to {}", git.branch),
      count => format!("Commit {count} files to {}", git.branch),
    };
    let commit_box = v_flex()
      .flex_none()
      .gap_2()
      .pr_3()
      .pb_3()
      .pt_2()
      .child(Input::new(&self.commit_summary))
      .child(
        div().relative().child(Textarea::new(&self.commit_description).h(px(88.))).child(
          h_flex()
            .absolute()
            .right_1()
            .bottom_1()
            .gap_0p5()
            .child(
              Button::new("commit-settings")
                .ghost()
                .xsmall()
                .icon(Icon::empty().path("icons/settings-2.svg"))
                .tooltip("OpenCommit settings")
                .on_click(cx.listener(|this, _, window, cx| this.open_commit_settings(window, cx))),
            )
            .child(
              Button::new("generate-message")
                .ghost()
                .xsmall()
                .icon(Icon::empty().path("icons/sparkles.svg"))
                .loading(self.generating)
                .disabled(chosen == 0 || self.generating)
                .tooltip("Generate the summary and description with OpenCommit")
                .on_click(cx.listener(|this, _, window, cx| this.generate_commit_message(window, cx))),
            ),
        ),
      )
      .child(
        Button::new("commit")
          .primary()
          .w_full()
          .label(label)
          .disabled(chosen == 0 || !summary_filled || self.committing)
          .on_click(cx.listener(|this, _, window, cx| this.commit_changes(window, cx))),
      );
    let tabs = TabBar::new("git-view-tabs")
      .underline()
      .selected_index(usize::from(self.git_view == GitView::History))
      .on_click(cx.listener(|this, index: &usize, _, cx| this.set_git_view(if *index == 1 { GitView::History } else { GitView::Changes }, cx)))
      .children([Tab::new().label("Changes"), Tab::new().label("History")]);
    let body = match self.git_view {
      GitView::Changes => v_flex()
        .flex_1()
        .min_h_0()
        .gap_1()
        .child(
          h_flex()
            .px_2()
            .py_1()
            .gap_2()
            .items_center()
            .child(div().size_2().rounded_full().bg(if git.changed() > 0 { theme.primary } else { theme.muted_foreground }).mx_1())
            .child(Label::new(changed).text_color(theme.muted_foreground)),
        )
        .child(div().flex_1().min_h_0().when(git.changed() > 0, |list| list.child(changes_list)))
        .child(commit_box)
        .into_any_element(),
      GitView::History => self.render_history(cx).into_any_element(),
    };
    Some(v_flex().size_full().gap_1().child(div().pr_3().child(tabs)).child(body))
  }

  /// The commits of the open project, newest first: the subject, who made it and when, the tags on it, and an arrow while the
  /// remote does not have it yet. Choosing one shows it in the main area.
  fn render_history(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let body = if self.compare_open {
      self.render_compare_choices(cx)
    } else if let Some(view) = &self.compare {
      self.render_compare(view, cx)
    } else {
      self.commit_list(&self.history, true, cx)
    };
    v_flex().flex_1().min_h_0().gap_2().child(self.render_compare_field(cx)).child(body)
  }

  /// A list of commits: the subject, who made it and when, the tags on it, and an arrow while the remote does not have it yet.
  /// With `paged` the next page of the History view is loaded when the list is scrolled to its end.
  fn commit_list(&self, commits: &[CommitInfo], paged: bool, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    if commits.is_empty() {
      return Label::new(if paged { "No commits yet" } else { "No commits" }).px_2().py_2().text_color(theme.muted_foreground).into_any_element();
    }
    let workspace = cx.entity();
    let tip = self.history.first().map(|commit| commit.id.clone());
    let rows = commits.iter().map(|commit| {
      let id = commit.id.clone();
      let selected = self.selected_commit.as_deref() == Some(commit.id.as_str());
      // Only the last commit, and only while the remote does not have it, can be undone.
      let undoable = tip.as_deref() == Some(commit.id.as_str()) && commit.unpushed;
      let (menu_workspace, menu_id) = (workspace.clone(), commit.id.clone());
      let initial: String = commit.author.chars().take(1).flat_map(char::to_uppercase).collect();
      h_flex()
        .id(SharedString::from(format!("commit-{}", commit.id)))
        .gap_2()
        .px_2()
        .py_1p5()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |row| row.bg(palette.selected))
        .hover(|row| row.bg(palette.hover))
        .child(
          v_flex()
            .flex_1()
            .min_w_0()
            .gap_0p5()
            .child(Label::new(commit.summary.clone()).font_weight(FontWeight::MEDIUM).overflow_hidden().whitespace_nowrap().text_ellipsis())
            .child(
              h_flex()
                .gap_1p5()
                .items_center()
                .child(
                  div()
                    .size_4()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(palette.hover)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(initial),
                )
                .child(
                  Label::new(format!("{} · {}", commit.author, ago_long(commit.time)))
                    .text_size(FONT_SIZE_SM)
                    .text_color(theme.muted_foreground)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis(),
                ),
            ),
        )
        .children(
          commit
            .tags
            .iter()
            .take(2)
            .map(|tag| div().px_1p5().rounded_md().bg(palette.hover).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground).child(tag.clone())),
        )
        .when(commit.unpushed, |row| {
          row.child(
            div()
              .id(SharedString::from(format!("unpushed-{}", commit.id)))
              .size_5()
              .flex()
              .flex_none()
              .items_center()
              .justify_center()
              .rounded_full()
              .bg(palette.hover)
              .tooltip(|window, cx| Tooltip::new("Not pushed yet").build(window, cx))
              .child(Icon::empty().path("icons/arrow-up.svg").size_4().text_color(theme.muted_foreground)),
          )
        })
        .on_click(cx.listener(move |this, _, _, cx| this.select_commit(id.clone(), cx)))
        .context_menu(move |menu, _, _| {
          let (undo_workspace, undo_id) = (menu_workspace.clone(), menu_id.clone());
          let (tag_workspace, tag_id) = (menu_workspace.clone(), menu_id.clone());
          let copy_id = menu_id.clone();
          menu
            .item(
              PopupMenuItem::new("Undo Commit…")
                .disabled(!undoable)
                .on_click(move |_, window, cx| undo_workspace.update(cx, |this, cx| this.undo_commit(undo_id.clone(), window, cx))),
            )
            .separator()
            .item(
              PopupMenuItem::new("Create Tag…")
                .on_click(move |_, window, cx| tag_workspace.update(cx, |this, cx| this.create_tag(tag_id.clone(), window, cx))),
            )
            .separator()
            .item(PopupMenuItem::new("Copy SHA").on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy_id.clone()))))
        })
    });
    // The next page of commits is loaded when the list is scrolled to its end (after the wheel stops), or with the button there.
    let more = (paged && self.history.len() >= self.history_limit).then(|| {
      Button::new("history-more")
        .ghost()
        .small()
        .label(format!("Load {HISTORY_PAGE} more commits"))
        .on_click(cx.listener(|this, _, _, cx| this.load_more_history(false, cx)))
    });
    div()
      .relative()
      .flex_1()
      .min_h_0()
      .child(
        v_flex()
          .id("history-list")
          .size_full()
          .pr_3()
          .overflow_y_scroll()
          .track_scroll(&self.history_scroll)
          .on_scroll(cx.listener(move |_, _: &ScrollEvent, _, cx| {
            if !paged {
              return;
            }
            cx.spawn(async move |this, cx| {
              cx.background_executor().timer(HISTORY_SCROLL_SETTLE).await;
              let _ = this.update(cx, |this, cx| this.load_more_history(true, cx));
            })
            .detach();
          }))
          .children(rows)
          .children(more),
      )
      .child(div().id("history-scrollbar").absolute().size_full().vertical_scrollbar(&self.history_scroll))
      .into_any_element()
  }

  /// The field above the History list: choose a branch to compare with, or the branch chosen, with a button to stop comparing.
  fn render_compare_field(&self, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let branch_icon = || Icon::empty().path("icons/git-branch.svg").size_4().text_color(theme.muted_foreground);
    let field = match &self.compare {
      Some(view) => h_flex()
        .h_8()
        .px_2()
        .gap_2()
        .items_center()
        .rounded_md()
        .border_1()
        .border_color(theme.input)
        .child(branch_icon())
        .child(Label::new(view.branch.clone()).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
        .child(
          Button::new("compare-clear")
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/x.svg").size_4())
            .tooltip("Stop comparing")
            .on_click(cx.listener(|this, _, _, cx| this.clear_compare(cx))),
        )
        .into_any_element(),
      None => h_flex()
        .gap_1()
        .items_center()
        .child(div().flex_1().child(Input::new(&self.compare_filter).prefix(branch_icon())))
        .when(self.compare_open, |row| {
          row.child(
            Button::new("compare-close")
              .ghost()
              .xsmall()
              .icon(Icon::empty().path("icons/x.svg").size_4())
              .tooltip("Close")
              .on_click(cx.listener(|this, _, window, cx| this.close_compare_list(window, cx))),
          )
        })
        .into_any_element(),
    };
    div().pr_3().child(field).into_any_element()
  }

  /// The branches under the compare field: the default one, the ones used lately and all the others, each with how many commits
  /// it is behind (↓) and ahead (↑) of the current branch.
  fn render_compare_choices(&self, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    if self.compare_choices.is_none() {
      return h_flex()
        .gap_2()
        .p_3()
        .items_center()
        .justify_center()
        .child(Spinner::new().small())
        .child(Label::new("Reading branches…").text_color(theme.muted_foreground))
        .into_any_element();
    }
    let (default, recent, other) = self.matching_compare_branches(cx);
    if default.is_none() && recent.is_empty() && other.is_empty() {
      return Label::new("No matching branches").px_2().py_1().text_color(theme.muted_foreground).into_any_element();
    }
    let heading = |text: &'static str| Label::new(text).px_2().pt_2().pb_1().font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground);
    let row = |branch: &BranchRef, cx: &mut Context<Self>| {
      let name = branch.name.clone();
      h_flex()
        .id(SharedString::from(format!("compare-branch-{}", branch.name)))
        .gap_2()
        .px_2()
        .py_1p5()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .hover(|row| row.bg(palette.hover))
        .child(Icon::empty().path("icons/git-branch.svg").size_4().text_color(theme.muted_foreground))
        .child(Label::new(branch.name.clone()).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
        .child(Label::new(format!("{}↓ {}↑", branch.behind, branch.ahead)).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
        .on_click(cx.listener(move |this, _, window, cx| this.choose_compare_branch(name.clone(), window, cx)))
        .into_any_element()
    };
    let mut list = v_flex().id("compare-list").flex_1().min_h_0().pr_3().overflow_y_scrollbar();
    if let Some(branch) = &default {
      list = list.child(heading("Default Branch")).child(row(branch, cx));
    }
    if !recent.is_empty() {
      list = list.child(heading("Recent Branches")).children(recent.iter().map(|branch| row(branch, cx)).collect::<Vec<_>>());
    }
    if !other.is_empty() {
      list = list.child(heading("Other Branches")).children(other.iter().map(|branch| row(branch, cx)).collect::<Vec<_>>());
    }
    list.into_any_element()
  }

  /// The comparison with the chosen branch: its two sides as tabs, the commits of the shown side, and what merging would do.
  fn render_compare(&self, view: &CompareView, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let current = self.git.as_ref().map(|git| git.branch.clone()).unwrap_or_default();
    let (behind, ahead) = view.found.as_ref().map_or((0, 0), |found| (found.behind_count, found.ahead_count));
    let tabs = TabBar::new("compare-sides")
      .segmented()
      .selected_index(usize::from(view.side == CompareSide::Ahead))
      .on_click(
        cx.listener(|this, index: &usize, _, cx| this.set_compare_side(if *index == 1 { CompareSide::Ahead } else { CompareSide::Behind }, cx)),
      )
      .children([Tab::new().label(format!("Behind ({behind})")), Tab::new().label(format!("Ahead ({ahead})"))]);
    let list = match &view.found {
      None => h_flex()
        .gap_2()
        .p_3()
        .items_center()
        .justify_center()
        .child(Spinner::new().small())
        .child(Label::new("Comparing…").text_color(theme.muted_foreground))
        .into_any_element(),
      Some(found) => {
        let commits = match view.side {
          CompareSide::Behind => &found.behind,
          CompareSide::Ahead => &found.ahead,
        };
        div().flex_1().min_h_0().flex().flex_col().child(self.commit_list(commits, false, cx)).into_any_element()
      }
    };
    let bold = |text: String| Label::new(text).font_weight(FontWeight::SEMIBOLD);
    let note = |parts: Vec<AnyElement>| {
      h_flex().flex_wrap().justify_center().gap_1().text_size(FONT_SIZE_SM).text_color(theme.muted_foreground).children(parts)
    };
    let footer = view.found.as_ref().map(|_| {
      let message = if behind > 0 {
        note(vec![
          Label::new("This will merge").into_any_element(),
          bold(format!("{behind} commit{}", if behind == 1 { "" } else { "s" })).into_any_element(),
          Label::new("from").into_any_element(),
          bold(view.branch.clone()).into_any_element(),
          Label::new("into").into_any_element(),
          bold(current.clone()).into_any_element(),
        ])
      } else {
        note(vec![Label::new("This branch is up to date with").into_any_element(), bold(view.branch.clone()).into_any_element()])
      };
      v_flex().gap_2().pr_3().pb_3().pt_2().items_center().child(Icon::new(IconName::Check).size_4().text_color(palette.new)).child(message).when(
        behind > 0,
        |footer| {
          footer.child(
            div().w_full().child(
              Button::new("compare-merge")
                .primary()
                .w_full()
                .label("Create a merge commit")
                .loading(self.merging)
                .disabled(self.merging)
                .on_click(cx.listener(|this, _, window, cx| this.merge_compare(window, cx))),
            ),
          )
        },
      )
    });
    v_flex().flex_1().min_h_0().gap_2().child(div().pr_3().child(tabs)).child(list).children(footer).into_any_element()
  }

  /// One line of the folder tree.
  fn explorer_row(&self, row: &explorer::Row, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let path = row.path.clone();
    let selected = self.selected_entry.as_ref() == Some(&row.path);
    let open = self.expanded.contains(&row.path);
    h_flex()
      .id(SharedString::from(format!("entry-{}", row.path.display())))
      .w_full()
      .gap_1p5()
      .pr_2()
      .py_1()
      .pl(INDENT * row.depth as f32 + px(8.))
      .rounded_md()
      .items_center()
      .cursor_pointer()
      .when(selected, |entry| entry.bg(palette.selected))
      .hover(|entry| entry.bg(palette.hover))
      .child(div().size_4().flex_none().when(row.is_dir, |chevron| {
        chevron.child(Icon::new(if open { IconName::ChevronDown } else { IconName::ChevronRight }).size_4().text_color(theme.muted_foreground))
      }))
      // Only folders get an icon; a file's name starts where a folder's icon does.
      .when(row.is_dir, |entry| entry.child(Icon::new(IconName::FolderClosed).size_4().text_color(theme.muted_foreground)))
      .child(Label::new(row.name.clone()).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
      .when(!row.is_dir, |entry| {
        let hovered_path = path.clone();
        entry.on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
          if !*hovered {
            if this.hovered_file.as_ref() == Some(&hovered_path) {
              this.hovered_file = None;
            }
            return;
          }
          this.hovered_file = Some(hovered_path.clone());
          let wanted = hovered_path.clone();
          cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(HOVER_PREPARE_DELAY).await;
            let _ = this.update_in(cx, |this, window, cx| {
              if this.hovered_file.as_ref() == Some(&wanted) {
                this.editor_panel.update(cx, |editor, cx| editor.prefetch(&wanted, window, cx));
              }
            });
          })
          .detach();
        }))
      })
      .on_click(cx.listener(move |this, _, window, cx| {
        if this.rows.iter().any(|row| row.path == path && row.is_dir) {
          this.selected_entry = Some(path.clone());
          this.toggle_folder(path.clone(), cx);
        } else {
          this.open_file(path.clone(), window, cx);
        }
      }))
  }

  fn render_explorer(&self, cx: &mut Context<Self>) -> impl IntoElement {
    // Only the lines in view are built, laid out and drawn: a project can have thousands of entries.
    let list = uniform_list(
      "explorer-rows",
      self.rows.len(),
      cx.processor(|this, range: Range<usize>, _, cx| {
        let rows: Vec<explorer::Row> = this.rows.get(range).map(<[explorer::Row]>::to_vec).unwrap_or_default();
        // The padding on the right is inside each line, so that the scrollbar sits at the edge of the sidebar.
        rows.iter().map(|row| div().w_full().pr_3().child(this.explorer_row(row, cx)).into_any_element()).collect::<Vec<_>>()
      }),
    )
    .track_scroll(&self.explorer_scroll)
    .size_full();
    let rows = div().relative().flex_1().min_h_0().child(list).child(
      div()
        .absolute()
        .inset_0()
        .child(Scrollbar::new(&self.explorer_scroll).id("explorer-scrollbar").axis(ScrollbarAxis::Vertical).viewport_from_layout()),
    );
    let name = self
      .projects
      .projects
      .iter()
      .find(|project| self.projects.active.as_ref() == Some(&project.path))
      .map_or_else(|| SharedString::from("Vekro"), |project| SharedString::from(project.name()));
    let action =
      |id: &'static str, icon: &'static str, tooltip: &'static str| Button::new(id).ghost().xsmall().icon(Icon::empty().path(icon)).tooltip(tooltip);
    let header = h_flex()
      .gap_1()
      .items_center()
      .pl_2()
      .pr_3()
      .child(Label::new(name).flex_1().min_w_0().font_weight(FontWeight::SEMIBOLD).whitespace_nowrap().overflow_hidden().text_ellipsis())
      .child(
        action("explorer-new-file", "icons/file-plus.svg", "New file").on_click(cx.listener(|this, _, window, cx| this.new_entry(false, window, cx))),
      )
      .child(
        action("explorer-new-folder", "icons/folder-plus.svg", "New folder")
          .on_click(cx.listener(|this, _, window, cx| this.new_entry(true, window, cx))),
      )
      .child(action("explorer-refresh", "icons/refresh-cw.svg", "Refresh").on_click(cx.listener(|this, _, _, cx| {
        this.refresh_project();
        cx.notify();
      })))
      .child(
        action("explorer-collapse", "icons/list-chevrons-down-up.svg", "Collapse folders")
          .on_click(cx.listener(|this, _, _, cx| this.collapse_folders(cx))),
      );
    v_flex().flex_1().min_h_0().gap_2().child(header).child(rows)
  }

  /// The repository summary and the folder tree.
  fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let content = match self.sidebar_tab {
      SidebarTab::Explorer => self.render_explorer(cx).into_any_element(),
      SidebarTab::Git => match self.render_git(cx) {
        Some(git) => git.into_any_element(),
        None => Label::new("Not a Git repository").text_color(cx.theme().muted_foreground).px_2().into_any_element(),
      },
    };
    v_flex().size_full().bg(cx.theme().sidebar).child(v_flex().flex_1().min_h_0().pl_3().pt_3().child(content))
  }

  /// The narrow bar at the far left: one button for each view of the sidebar (Explorer, Git), the open one marked; the terminal
  /// button under them; and Settings at the bottom, with a small warning at its corner while an account needs attention.
  fn render_activity_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let workspace = cx.entity();
    let changed = self.git.as_ref().map_or(0, Summary::changed);
    let buttons = SidebarTab::ALL.map(|tab| {
      let workspace = workspace.clone();
      let button =
        rail_button(tab.label(), tab.icon(), tab.label(), tab == self.sidebar_tab && !self.terminal_open, true, &palette, move |_, window, cx| {
          workspace.update(cx, |this, cx| {
            this.sidebar_tab = tab;
            // The view of the sidebar and the editor come back when the terminals were taking the main area.
            if this.terminal_open {
              this.hide_terminal(window, cx);
            }
            cx.notify();
          })
        });
      // The Git button shows how many files have changes, in a small blue circle at its top right.
      let badge = (tab == SidebarTab::Git && changed > 0).then(|| {
        div()
          .absolute()
          .top_0()
          .right(-BADGE_OVERHANG)
          .min_w_4()
          .h_4()
          .px_1()
          .flex()
          .items_center()
          .justify_center()
          .rounded_full()
          .bg(theme.primary)
          .text_color(palette.on_accent)
          .text_size(BADGE_FONT_SIZE)
          .child(if changed > 99 { "99+".to_string() } else { changed.to_string() })
      });
      div().relative().child(button).children(badge)
    });
    let attention = self.settings.read(cx).accounts.items.iter().any(|account| account.issue.is_some());
    let terminal_attention = self.terminal_tabs.values().any(|tabs| tabs.panes().iter().any(|terminal| terminal.read(cx).needs_attention()));
    let terminal =
      rail_button("terminal-toggle", Icon::empty().path("icons/terminal.svg"), "Terminal", self.terminal_open, true, &palette, |_, window, cx| {
        window.dispatch_action(Box::new(ToggleTerminal), cx)
      });
    let settings = div()
      .relative()
      .child(rail_button("settings-open", Icon::new(IconName::Settings), "Settings", false, true, &palette, |_, window, cx| {
        window.dispatch_action(Box::new(OpenSettings), cx)
      }))
      // While an account has a permission problem, a small warning sits at the corner of the gear and opens Accounts.
      .when(attention, |gear| {
        gear.child(
          div()
            .id("account-attention")
            .absolute()
            .bottom_0()
            .right_0()
            .size_4()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .bg(theme.warning)
            .tooltip(|window, cx| Tooltip::new("An account needs attention").build(window, cx))
            .child(Icon::empty().path("icons/triangle-alert.svg").size_3().text_color(palette.on_accent))
            .on_click(cx.listener(|this, _, window, cx| this.open_settings_page("Accounts", window, cx))),
        )
      });
    v_flex()
      .w(ACTIVITY_BAR_WIDTH)
      .h_full()
      .flex_none()
      .items_center()
      .py_2()
      .gap_1()
      .bg(palette.rail)
      .border_r_1()
      .border_color(palette.border)
      .children(buttons)
      .child(div().relative().child(terminal).when(terminal_attention, |button| {
        button.child(
          div()
            .absolute()
            .bottom_0()
            .right_0()
            .size_4()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(theme.warning)
            .child(Icon::empty().path("icons/triangle-alert.svg").size_3().text_color(palette.on_accent)),
        )
      }))
      .child(div().flex_1())
      .child(settings)
  }

  /// The terminals, filling the main area like a terminal window: a tab strip when there are several tabs, and the panes of the
  /// shown tab, split side by side or one above the other, each with a small header when there is more than one.
  fn render_terminal_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let (Some(root), Some(arrangement)) = (self.projects.active.clone(), self.arrangement()) else {
      return Label::new("No project is open.").text_color(theme.muted_foreground).p_3().into_any_element();
    };
    let Some(node) = arrangement.current().cloned() else {
      return Label::new("No terminal is open.").text_color(theme.muted_foreground).p_3().into_any_element();
    };
    let strip = (arrangement.tabs.len() > 1).then(|| {
      let tabs = arrangement.tabs.iter().enumerate().map(|(index, tab)| {
        let first = tab.panes().first().cloned();
        let label = first.as_ref().map(|terminal| terminal.read(cx).title()).unwrap_or_default();
        let key = first.map_or_else(|| index.to_string(), |terminal| terminal.entity_id().to_string());
        let close = Button::new(SharedString::from(format!("close-terminal-tab-{key}")))
          .ghost()
          .xsmall()
          .icon(Icon::empty().path("icons/x.svg").size_4())
          .tooltip("Close")
          .on_click(cx.listener(move |this, _, window, cx| {
            // The click must not also reach the tab and select it.
            cx.stop_propagation();
            this.close_terminal_tab(index, window, cx);
          }));
        strip_tab(SharedString::from(format!("terminal-tab-{key}")), label, index == arrangement.active, close, cx)
          .on_click(cx.listener(move |this, _, window, cx| this.select_terminal_tab(index, window, cx)))
      });
      strip(cx).child(h_flex().flex_1().min_w_0().h_full().children(tabs)).child(
        Button::new("terminal-new-tab")
          .ghost()
          .xsmall()
          .icon(Icon::empty().path("icons/plus.svg").size_4())
          .tooltip("New terminal tab")
          .mx_2()
          .on_click(cx.listener(|this, _, window, cx| this.new_terminal(&NewTerminal, window, cx))),
      )
    });
    let headers = node.panes().len() > 1;
    v_flex()
      .size_full()
      .children(strip)
      .child(div().flex_1().min_h_0().child(self.render_pane_node(&node, headers, &root.to_string_lossy(), cx)))
      .into_any_element()
  }

  /// A part of a tab: a pane, or two parts split by a divider that can be dragged.
  fn render_pane_node(&self, node: &Node<Entity<Terminal>>, headers: bool, key: &str, cx: &mut Context<Self>) -> AnyElement {
    match node {
      Node::Pane(terminal) => self.render_pane(terminal, headers, cx),
      Node::Split { id, direction, first, second } => {
        let id = SharedString::from(format!("terminal-split-{key}-{id}"));
        let group = match direction {
          Direction::Columns => h_resizable(id),
          Direction::Rows => v_resizable(id),
        };
        group
          .child(resizable_panel().child(self.render_pane_node(first, headers, key, cx)))
          .child(resizable_panel().child(self.render_pane_node(second, headers, key, cx)))
          .into_any_element()
      }
    }
  }

  /// One shell, with a header (close, title, menu) when the tab has several panes.
  fn render_pane(&self, terminal: &Entity<Terminal>, headers: bool, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let workspace = cx.entity();
    let header = headers.then(|| {
      let (close, menu_terminal) = (terminal.clone(), terminal.clone());
      let (close_workspace, menu_workspace) = (workspace.clone(), workspace.clone());
      h_flex()
        .id(SharedString::from(format!("terminal-header-{}", terminal.entity_id())))
        .flex_none()
        .gap_1()
        .px_1()
        .py_0p5()
        .items_center()
        .bg(palette.surface_alt)
        .border_b_1()
        .border_color(palette.border)
        .child(
          Button::new(SharedString::from(format!("terminal-close-{}", terminal.entity_id())))
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/x.svg").size_4())
            .tooltip("Close")
            .on_click(move |_, window, cx| close_workspace.update(cx, |this, cx| this.close_pane(close.clone(), window, cx))),
        )
        .child(
          Label::new(terminal.read(cx).title())
            .flex_1()
            .min_w_0()
            .text_size(FONT_SIZE_SM)
            .text_color(theme.muted_foreground)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis(),
        )
        .child(
          Button::new(SharedString::from(format!("terminal-menu-{}", terminal.entity_id())))
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/ellipsis.svg").size_4())
            .tooltip("More")
            .dropdown_menu(move |menu, _, _| {
              let (tab, vertical, horizontal, close) =
                (menu_workspace.clone(), menu_workspace.clone(), menu_workspace.clone(), menu_workspace.clone());
              let (vertical_terminal, horizontal_terminal, close_terminal) = (menu_terminal.clone(), menu_terminal.clone(), menu_terminal.clone());
              menu
                .item(
                  PopupMenuItem::new("New Tab").on_click(move |_, window, cx| tab.update(cx, |this, cx| this.new_terminal(&NewTerminal, window, cx))),
                )
                .separator()
                .item(PopupMenuItem::new("Split Pane Vertically").on_click(move |_, window, cx| {
                  vertical.update(cx, |this, cx| this.split_pane(vertical_terminal.clone(), Direction::Columns, window, cx))
                }))
                .item(PopupMenuItem::new("Split Pane Horizontally").on_click(move |_, window, cx| {
                  horizontal.update(cx, |this, cx| this.split_pane(horizontal_terminal.clone(), Direction::Rows, window, cx))
                }))
                .separator()
                .item(
                  PopupMenuItem::new("Close")
                    .on_click(move |_, window, cx| close.update(cx, |this, cx| this.close_pane(close_terminal.clone(), window, cx))),
                )
            }),
        )
    });
    v_flex().size_full().children(header).child(div().flex_1().min_h_0().child(terminal.clone())).into_any_element()
  }
}

/// One project in a list (a category's list, the repository menu): its avatar and name; a click opens it and closes the list,
/// a right click opens the project's menu.
fn project_list_row(workspace: &Entity<Workspace>, project: &Project, selected: bool, status: Option<&ProjectStatus>, cx: &App) -> AnyElement {
  let palette = colors(cx);
  let look = avatar_look(project, selected, cx);
  let (path, name) = (project.path.clone(), SharedString::from(project.name()));
  let (favorite, star_workspace, star_path) = (project.favorite, workspace.clone(), project.path.clone());
  let muted = cx.theme().muted_foreground;
  h_flex()
    .id(SharedString::from(format!("project-row-{}", path.display())))
    .group("project-row")
    .gap_2()
    .px_2()
    .py_1()
    .rounded_md()
    .items_center()
    .cursor_pointer()
    .when(selected, |row| row.bg(palette.selected))
    .hover(|row| row.bg(palette.hover))
    // The star, at the left: filled and always shown for a favorite, an outline that appears while the pointer is on the row otherwise.
    .child(
      div()
        .id(SharedString::from(format!("project-star-{}", path.display())))
        .size_6()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .hover(|star| star.bg(palette.hover))
        .when(!favorite, |star| star.opacity(0.5))
        .tooltip(move |window, cx| Tooltip::new(if favorite { "Remove from favorites" } else { "Add to favorites" }).build(window, cx))
        .child(Icon::empty().path(if favorite { "icons/star-fill.svg" } else { "icons/star.svg" }).size_4().text_color(if favorite {
          palette.modified
        } else {
          muted
        }))
        .on_click(move |_, _, cx| {
          // The click must not also open the project.
          cx.stop_propagation();
          star_workspace.update(cx, |this, cx| this.toggle_favorite(&star_path, cx));
        }),
    )
    .child(
      div()
        .size_6()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_sm()
        .text_color(look.text)
        .when_some(look.fill, |avatar, fill| avatar.bg(fill))
        .when_some(look.icon.clone(), |avatar, icon| avatar.child(Icon::empty().path(icon).size_4()))
        .when(look.icon.is_none(), |avatar| avatar.child(look.letter.clone())),
    )
    .child(Label::new(name).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
    // What is waiting in the project: commits to push (an arrow), and files with changes (a blue dot).
    .when(status.is_some_and(|status| status.push), |row| {
      row.child(
        div()
          .id(SharedString::from(format!("project-push-{}", path.display())))
          .flex()
          .flex_none()
          .items_center()
          .justify_center()
          .px_2()
          .rounded_full()
          .bg(palette.hover)
          .tooltip(|window, cx| Tooltip::new("Commits waiting to be pushed").build(window, cx))
          .child(Icon::empty().path("icons/arrow-up.svg").size_3p5().text_color(muted)),
      )
    })
    .when(status.is_some_and(|status| status.changed > 0), |row| {
      let changed = status.map_or(0, |status| status.changed);
      row.child(
        div()
          .id(SharedString::from(format!("project-changes-{}", path.display())))
          .flex_none()
          .size_2()
          .rounded_full()
          .bg(cx.theme().primary)
          .tooltip(move |window, cx| {
            Tooltip::new(if changed == 1 { "1 uncommitted change".to_string() } else { format!("{changed} uncommitted changes") }).build(window, cx)
          }),
      )
    })
    .on_click({
      let workspace = workspace.clone();
      move |_, window, cx| {
        workspace.update(cx, |this, cx| {
          this.projects_menu_open = false;
          this.select_project(path.clone(), window, cx);
        })
      }
    })
    .into_any_element()
}

/// What a failed push, pull or fetch says about the account's token, if it says anything: the host turned the token down, or
/// accepted it but refused commits that change workflow files.
fn refusal_kind(message: &str) -> Option<IssueKind> {
  if message.contains(vekro_git::WORKFLOWS_REFUSED) {
    Some(IssueKind::Workflows)
  } else if message.contains(vekro_git::TOKEN_REFUSED) {
    Some(IssueKind::Push)
  } else {
    None
  }
}

/// How long ago a commit was made, in words: "2 minutes ago", "3 hours ago", "yesterday".
fn ago_long(seconds_since_epoch: i64) -> String {
  let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs() as i64);
  let seconds = (now - seconds_since_epoch).max(0);
  let plural = |count: i64, unit: &str| if count == 1 { format!("1 {unit} ago") } else { format!("{count} {unit}s ago") };
  match seconds {
    0..=59 => "just now".to_string(),
    60..=3599 => plural(seconds / 60, "minute"),
    3600..=86399 => plural(seconds / 3600, "hour"),
    86400..=172799 => "yesterday".to_string(),
    172800..=2_591_999 => plural(seconds / 86400, "day"),
    2_592_000..=31_535_999 => plural(seconds / 2_592_000, "month"),
    _ => plural(seconds / 31_536_000, "year"),
  }
}

/// How long ago `time` was, for a label: "just now", "5m ago", "3h ago", "2d ago".
fn time_ago(time: std::time::SystemTime) -> String {
  let seconds = time.elapsed().map_or(0, |elapsed| elapsed.as_secs());
  match seconds {
    0..=59 => "just now".to_string(),
    60..=3599 => format!("{}m ago", seconds / 60),
    3600..=86399 => format!("{}h ago", seconds / 3600),
    _ => format!("{}d ago", seconds / 86400),
  }
}

/// What a category is called in a menu or a tooltip: its name, or its number while it has none.
fn category_label(index: usize, category: &Category) -> String {
  if category.name.is_empty() {
    format!("Category {}", index + 1)
  } else {
    category.name.clone()
  }
}

/// A square icon button for the title bar, as tall as the repository switcher. The Kit's `Button` fixes its icon at a small
/// size, so this draws the icon itself at `size_5`.
fn rail_button(
  id: &'static str,
  icon: Icon,
  tooltip: &'static str,
  selected: bool,
  large: bool,
  palette: &crate::theme::Colors,
  on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
  let hover = palette.rail_hover;
  div()
    .id(id)
    .map(|button| if large { button.size_10() } else { button.size_8() })
    .flex()
    .flex_none()
    .items_center()
    .justify_center()
    .rounded_md()
    .cursor_pointer()
    .text_color(palette.rail_text)
    .when(selected, |button| button.bg(hover))
    .hover(move |button| button.bg(hover))
    .child(icon.size_5())
    .tooltip(move |window, cx| Tooltip::new(tooltip).build(window, cx))
    .on_click(on_click)
}

/// The color of a changed file's mark: green for new, red for deleted, amber for modified and so on.
fn kind_color(kind: vekro_git::ChangeKind, palette: &crate::theme::Colors) -> Hsla {
  match kind {
    vekro_git::ChangeKind::New => palette.new,
    vekro_git::ChangeKind::Modified => palette.modified,
    vekro_git::ChangeKind::Deleted => palette.deleted,
    vekro_git::ChangeKind::Renamed => palette.renamed,
    vekro_git::ChangeKind::Conflicted => palette.conflicted,
  }
}

impl Render for Workspace {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let commit_shown = self.sidebar_tab == SidebarTab::Git && self.git_view == GitView::History && self.selected_commit.is_some();
    let change_shown = self.sidebar_tab == SidebarTab::Git && self.git_view == GitView::Changes && self.selected_change.is_some();
    let editor_area: AnyElement = if commit_shown {
      self.commit_view.clone().into_any_element()
    } else if change_shown {
      self.change_view.clone().into_any_element()
    } else {
      self.dock.clone().into_any_element()
    };
    // The terminals take the whole main area, like a terminal window; the sidebar stays beside it.
    let main_area = if self.terminal_open { self.render_terminal_area(cx).into_any_element() } else { editor_area };
    v_flex()
      .id("workspace")
      .key_context("Workspace")
      .track_focus(&self.focus_handle)
      .on_action(cx.listener(Self::add_project))
      .on_action(cx.listener(Self::open_settings))
      .on_action(cx.listener(Self::new_terminal))
      .on_action(cx.listener(Self::toggle_terminal))
      .on_action(cx.listener(Self::split_right))
      .on_action(cx.listener(Self::view_on_host))
      .on_action(cx.listener(Self::split_down))
      .on_action(cx.listener(Self::save_file))
      .on_action(cx.listener(Self::close_tab))
      .size_full()
      .bg(theme.background)
      .text_color(theme.foreground)
      .text_size(FONT_SIZE)
      .child(TitleBar::new().h(TOOLBAR_HEIGHT).pl(px(96.)).bg(palette.rail).border_b_1().border_color(palette.border).child(
        h_flex().size_full().items_center().gap_2().pr_3().child(self.render_repository_switcher(cx)).child(rail_button(
          "title-add",
          Icon::new(IconName::Plus),
          "Add project",
          false,
          false,
          &palette,
          |_, window, cx| window.dispatch_action(Box::new(AddProject), cx),
        )),
      ))
      .child(h_flex().flex_1().min_h_0().child(self.render_activity_bar(cx)).child(if self.terminal_open {
        // The terminals take everything right of the activity bar: no sidebar and no status bar, like a terminal window.
        div().flex_1().min_w_0().h_full().child(main_area).into_any_element()
      } else {
        h_resizable("sidebar-split")
          // `fixed`: the sidebar keeps the width the user gave it when the window is resized; only the main area changes.
          .child(resizable_panel().size(SIDEBAR_WIDTH).size_range(SIDEBAR_MIN_WIDTH..SIDEBAR_MAX_WIDTH).fixed().child(self.render_sidebar(cx)))
          // The status bar belongs to the editor: the sidebar runs down to the bottom of the window.
          .child(
            resizable_panel().child(
              v_flex()
                .size_full()
                .when(self.sidebar_tab == SidebarTab::Git, |column| column.children(self.render_git_toolbar(cx)))
                .child(div().flex_1().min_h_0().child(main_area))
                // The Git view has the branch in its own bar: no status bar there.
                .when(self.sidebar_tab != SidebarTab::Git, |column| column.child(self.render_status_bar(cx))),
            ),
          )
          .into_any_element()
      }))
  }
}
