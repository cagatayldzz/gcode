use crate::{
  agents::{AgentKind, AgentTabs},
  all_changes::{AllChanges, OpenChangedFile},
  change_view::ChangeView,
  commit_settings::{CommitSettings, CommitSettingsSaved},
  commit_view::CommitView,
  components::{avatar_look, display_path, strip, strip_tab},
  connect_account::{AccountConnected, ConnectAccount},
  connections::{accounts_path, credentials_for_repository, describe, AccountsList},
  editor::{ActiveTab, EditorPanel},
  explorer,
  merge_dialog::{MergeDialog, MergeEvent},
  project_manager::ProjectManager,
  project_search,
  project_settings::{ProjectSettings, ProjectSettingsEvent},
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
  AddProject, CheckForUpdates, CloseTab, NewClaude, NewCodex, NewTerminal, OpenSearchResults, OpenSettings, SaveFile, SearchProject, SplitDown,
  SplitRight, ToggleAgents, ToggleAllChanges, ToggleTerminal, ViewOnHost,
};
use gpui_kit::{base::Selectable as _, prelude::FluentBuilder as _};
use gpui_kit::{
  component::{
    button::{Button, ButtonCustomVariant, ButtonVariant, ButtonVariants as _},
    checkbox::Checkbox,
    dock::{panel_handle, DockArea, DockLayout, DockSkin},
    empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle},
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    label::Label,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    popover::Popover,
    resizable::{h_resizable, resizable_panel, v_resizable},
    scroll::{ScrollableElement as _, Scrollbar, ScrollbarAxis, ScrollbarMode},
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
  sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
  },
  time::{Duration, Instant},
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
/// How long the fill of a finished push, pull or fetch takes to reach the right edge.
const REMOTE_FINISH: Duration = Duration::from_millis(300);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

/// How long the pointer must rest on a file in the Explorer before it is prepared for opening, so that moving past
/// files costs nothing.
const HOVER_PREPARE_DELAY: Duration = Duration::from_millis(120);

/// How often the Git summary and the folder tree are read again while the window is open.
const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// How often the changes of the favorite projects are read for their tabs in the title bar.
const FAVORITE_STATUS_INTERVAL: Duration = Duration::from_secs(10);

/// Width of the sidebar when the window opens.
const SIDEBAR_WIDTH: Pixels = px(280.);

/// How narrow and how wide the sidebar can be dragged.
const SIDEBAR_MIN_WIDTH: Pixels = px(200.);
const SIDEBAR_MAX_WIDTH: Pixels = px(960.);

/// Indentation of each tree level.
const INDENT: Pixels = px(16.);

/// The views of the sidebar, in activity-bar order.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum SidebarTab {
  #[default]
  Explorer,
  Search,
  Git,
}

impl SidebarTab {
  const ALL: [SidebarTab; 3] = [SidebarTab::Explorer, SidebarTab::Search, SidebarTab::Git];

  fn icon(self) -> Icon {
    match self {
      SidebarTab::Explorer => Icon::empty().path("icons/folder-tree.svg"),
      SidebarTab::Search => Icon::new(IconName::Search),
      SidebarTab::Git => Icon::empty().path("icons/git-branch.svg"),
    }
  }

  fn label(self) -> &'static str {
    match self {
      SidebarTab::Explorer => "Explorer",
      SidebarTab::Search => "Search",
      SidebarTab::Git => "Git",
    }
  }
}

enum SearchListRow {
  File { first: usize, count: usize },
  Match(usize),
}

/// What the project menu shows beside a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectStatus {
  /// Files with uncommitted changes.
  changed: usize,
  /// Commits are waiting to be pushed.
  push: bool,
}

struct SearchCancellation(Arc<AtomicBool>);

impl Drop for SearchCancellation {
  fn drop(&mut self) {
    self.0.store(true, Ordering::Relaxed);
  }
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
  /// How far the work is, in thousandths, from the percentages git reports; it never goes back.
  fraction: Arc<AtomicU32>,
  started: std::time::Instant,
  fetching_after_push: Arc<AtomicBool>,
}

impl RemoteWork {
  /// How much of the button the fill covers: what git reported, or the slow creep forward while git says nothing (it connects, or
  /// there is nothing to transfer). The creep never passes 90% on its own.
  fn shown(&self) -> f32 {
    let creep = 0.9 * (1. - (-self.started.elapsed().as_secs_f32() / 3.).exp());
    (self.fraction.load(Ordering::Relaxed) as f32 / 1000.).max(creep).clamp(0., 1.)
  }
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
  search_open: bool,
  search_state: Entity<InputState>,
  search_subscriptions: Vec<Subscription>,
  search_replace: Entity<InputState>,
  search_include: Entity<InputState>,
  search_exclude: Entity<InputState>,
  search_options: project_search::Options,
  search_show_replace: bool,
  search_show_filters: bool,
  search_preserve_case: bool,
  search_occurrences: usize,
  search_files: usize,
  search_error: Option<SharedString>,
  replacement_task: Option<Task<()>>,
  search_collapsed: HashSet<PathBuf>,
  search_results: Vec<project_search::Match>,
  search_scroll: UniformListScrollHandle,
  search_task: Option<Task<()>>,
  search_cancellation: Option<SearchCancellation>,
  search_epoch: u64,
  /// The changed files left out of the next commit.
  excluded: HashSet<PathBuf>,
  commit_summary: Entity<InputState>,
  commit_description: Entity<TextareaState>,
  /// A commit is being made.
  committing: bool,
  discarding: bool,
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
  merge_task: Option<Task<()>>,
  merge_dialog: Option<(PathBuf, Entity<MergeDialog>, Subscription)>,
  selected_commit: Option<String>,
  commit_view: Entity<CommitView>,
  /// The changes of the chosen file of the Changes view, shown in the main area.
  change_view: Entity<ChangeView>,
  all_changes: Entity<AllChanges>,
  all_changes_open: bool,
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
  /// The AI view (Codex, Claude) fills the main area, like the terminals do; the two never show together.
  agent_open: bool,
  /// The agent sessions of each project; the sessions keep running while another project or view is shown.
  agent_tabs: HashMap<PathBuf, AgentTabs<Entity<Terminal>>>,
  /// A commit message is being generated.
  generating: bool,
  /// What each project holds, read when the project menu opens and while it is open.
  project_status: HashMap<PathBuf, ProjectStatus>,
  /// When the project status was last read in the background.
  project_status_read: Option<Instant>,
  /// The push, pull or fetch that is running.
  remote_work: Option<RemoteWork>,
  /// A finished push, pull or fetch whose fill is still running out to the right: where it stood, and which run it was.
  remote_finish: Option<(f32, usize)>,
  remote_runs: usize,
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
      search_open: false,
      search_state: cx.new(|cx| InputState::new(window, cx).placeholder("Search project files")),
      search_subscriptions: Vec::new(),
      search_replace: cx.new(|cx| InputState::new(window, cx).placeholder("Replace with")),
      search_include: cx.new(|cx| InputState::new(window, cx).placeholder("e.g. src/**, *.rs")),
      search_exclude: cx.new(|cx| InputState::new(window, cx).placeholder("e.g. tests/**")),
      search_options: Default::default(),
      search_show_replace: false,
      search_show_filters: false,
      search_preserve_case: false,
      search_occurrences: 0,
      search_files: 0,
      search_error: None,
      replacement_task: None,
      search_collapsed: HashSet::new(),
      search_results: Vec::new(),
      search_scroll: UniformListScrollHandle::default(),
      search_task: None,
      search_cancellation: None,
      search_epoch: 0,
      excluded: HashSet::new(),
      commit_summary: commit_summary.clone(),
      commit_description: commit_description.clone(),
      committing: false,
      discarding: false,
      branch_tab: BranchTab::Branches,
      git_view: GitView::Changes,
      history: Vec::new(),
      compare: None,
      compare_choices: None,
      compare_open: false,
      compare_filter: compare_filter.clone(),
      compare_epoch: 0,
      merge_task: None,
      merge_dialog: None,
      selected_commit: None,
      commit_view: cx.new(|_| CommitView::new()),
      change_view: cx.new(|_| ChangeView::new()),
      all_changes: cx.new(|_| AllChanges::default()),
      all_changes_open: false,
      pull_requests: PullRequests::Idle,
      pr_epoch: 0,
      history_epoch: 0,
      history_limit: HISTORY_PAGE,
      history_scroll: ScrollHandle::new(),
      window_active: true,
      menu_host: None,
      terminal_open: false,
      agent_open: false,
      agent_tabs: HashMap::new(),
      generating: false,
      project_status: HashMap::new(),
      project_status_read: None,
      remote_work: None,
      remote_finish: None,
      remote_runs: 0,
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
    self.start_program(root, None, "a shell", window, cx)
  }

  /// Starts `program` in `root` (the login shell for `None`); `what` names it in the notification when it cannot start.
  fn start_program(
    &mut self,
    root: PathBuf,
    program: Option<(String, Vec<String>)>,
    what: &str,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Option<Entity<Terminal>> {
    let started = match Terminal::start(root, program, cx) {
      Ok(started) => started,
      Err(error) => {
        window.push_notification(Notification::error(format!("Could not start {what}: {error}")), cx);
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
    self.agent_open = false;
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
    self.agent_open = false;
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

  /// Hides the terminals and the AI view; the programs keep running. The keyboard goes back to the window, so shortcuts keep working.
  pub(crate) fn hide_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.terminal_open = false;
    self.agent_open = false;
    self.focus_handle.focus(window, cx);
    cx.notify();
  }

  /// The agent sessions of the open project.
  fn agents(&self) -> Option<&AgentTabs<Entity<Terminal>>> {
    self.projects.active.as_ref().and_then(|root| self.agent_tabs.get(root))
  }

  /// Either full-width view is showing: the terminals or the AI view. The sidebar, the editor and the status bar are hidden then.
  fn full_area_open(&self) -> bool {
    self.terminal_open || self.agent_open
  }

  /// Gives the keyboard to the shown agent session, once it is drawn.
  fn focus_agent(&self, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(tab) = self.agents().and_then(AgentTabs::current) {
      self.focus_pane(&tab.session, window, cx);
    }
  }

  /// The activity bar's AI button and its shortcut: the AI view takes the main area, or gives it back to the editor.
  pub fn toggle_agents(&mut self, _: &ToggleAgents, window: &mut Window, cx: &mut Context<Self>) {
    if self.agent_open {
      self.hide_terminal(window, cx);
    } else {
      self.terminal_open = false;
      self.agent_open = true;
      self.focus_agent(window, cx);
      cx.notify();
    }
  }

  pub fn new_codex(&mut self, _: &NewCodex, window: &mut Window, cx: &mut Context<Self>) {
    self.new_agent(AgentKind::Codex, window, cx);
  }

  pub fn new_claude(&mut self, _: &NewClaude, window: &mut Window, cx: &mut Context<Self>) {
    self.new_agent(AgentKind::Claude, window, cx);
  }

  /// Starts an agent in a tab of its own in the open project and shows the AI view. It runs in the user's login shell, so
  /// the PATH of their shell finds it.
  fn new_agent(&mut self, kind: AgentKind, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let shell = std::env::var("SHELL").ok().filter(|shell| !shell.is_empty()).unwrap_or_else(|| "/bin/zsh".to_owned());
    let program = (shell, vec!["-l".to_owned(), "-i".to_owned(), "-c".to_owned(), kind.shell_line()]);
    let Some(terminal) = self.start_program(root.clone(), Some(program), kind.label(), window, cx) else { return };
    self.agent_tabs.entry(root).or_insert_with(AgentTabs::new).add(kind, terminal.clone());
    self.terminal_open = false;
    self.agent_open = true;
    self.focus_pane(&terminal, window, cx);
    cx.notify();
  }

  fn select_agent_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(root) = self.projects.active.clone() {
      if let Some(tabs) = self.agent_tabs.get_mut(&root) {
        tabs.select(index);
      }
    }
    self.focus_agent(window, cx);
    cx.notify();
  }

  /// Closes an agent session: the agent stops.
  fn close_agent_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(session) = self.agents().and_then(|tabs| tabs.tabs.get(index)).map(|tab| tab.session.clone()) {
      self.close_pane(session, window, cx);
    }
  }

  /// Takes a pane out, because its shell exited or it was closed: the shell stops, a tab left without panes goes, and when the
  /// open project has no shell left the main area goes back to the editor.
  pub(crate) fn close_pane(&mut self, terminal: Entity<Terminal>, window: &mut Window, cx: &mut Context<Self>) {
    for tabs in self.terminal_tabs.values_mut() {
      tabs.remove(&terminal);
    }
    self.terminal_tabs.retain(|_, tabs| !tabs.is_empty());
    for tabs in self.agent_tabs.values_mut() {
      tabs.remove(&terminal);
    }
    self.agent_tabs.retain(|_, tabs| !tabs.is_empty());
    // The AI view stays open when its last session ends: it shows the choice of agents again.
    if self.agent_open {
      self.focus_agent(window, cx);
    }
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

  /// Opens Settings on the Updates page and looks for a newer release.
  pub fn check_for_updates(&mut self, _: &CheckForUpdates, window: &mut Window, cx: &mut Context<Self>) {
    self.open_settings_page("Updates", window, cx);
    self.settings.update(cx, |model, cx| model.check_update(cx));
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
    window.open_dialog(cx, move |dialog, _, cx| {
      // Read on every draw, so that a theme change while the dialog is open recolors it.
      let palette = colors(cx);
      // The dialog's own padding is removed so that the page list reaches the edges of the window.
      dialog.p_0().width(px(820.)).child(
        v_flex()
          .rounded_b_lg()
          .overflow_hidden()
          .child(
            div().border_b_1().border_color(palette.border).child(Label::new("Settings").text_base().font_weight(FontWeight::SEMIBOLD).px_6().py_4()),
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
    window.open_dialog(cx, move |dialog, _, _| dialog.p_0().width(px(520.)).child(setup.clone()));
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
    self.confirm_project_removal(path, false, window, cx);
  }

  fn confirm_project_removal(&mut self, path: PathBuf, from_settings: bool, window: &mut Window, cx: &mut Context<Self>) {
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
          if from_settings {
            window.close_dialog(cx);
            window.close_dialog(cx);
          }
          workspace.update(cx, |this, cx| this.remove_project(&path, window, cx));
          !from_settings
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
    self.agent_tabs.remove(path);
    self.views.remove(path);
    cx.notify();
  }

  /// Opens the dialog for a project's name, icon, color and Git remote.
  pub(crate) fn open_project_settings(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
    let Some(project) = self.projects.projects.iter().find(|project| project.path == path).cloned() else { return };
    let settings = cx.new(|cx| ProjectSettings::new(&project, window, cx));
    self._dialog_subscription = Some(cx.subscribe_in(&settings, window, |this, _, event: &ProjectSettingsEvent, window, cx| {
      let chosen = match event {
        ProjectSettingsEvent::RemoveRequested(path) => {
          this.confirm_project_removal(path.clone(), true, window, cx);
          return;
        }
        ProjectSettingsEvent::Save(chosen) => chosen,
      };
      let mut path = chosen.path.clone();
      if chosen.rename_folder {
        match this.rename_project_folder(&chosen.path, chosen.name.trim(), cx) {
          Ok(moved) => path = moved,
          Err(message) => {
            window.push_notification(Notification::error(format!("Could not rename the folder: {message}")), cx);
            return;
          }
        }
      }
      this.projects.set_appearance(&path, &chosen.name, chosen.icon.as_deref(), chosen.color.as_deref());
      this.persist_projects();
      if chosen.rename_folder {
        this.terminal_tabs.remove(&chosen.path);
        this.agent_tabs.remove(&chosen.path);
        this.views.remove(&chosen.path);
        if this.projects.active.as_ref() == Some(&path) {
          this.refresh_project();
          this.ensure_terminal(window, cx);
        }
      }
      if let Some(url) = &chosen.remote {
        match vekro_git::set_remote_url(&path, url) {
          Ok(()) => {
            if this.projects.active.as_ref() == Some(&path) {
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
    window.open_dialog(cx, move |dialog, window, _| dialog.p_0().width(window.rem_size() * 38.).child(settings.clone()));
  }

  /// Renames a project's folder on disk and points the project at it. Returns the new path. Nothing changes when the
  /// name is not a plain folder name, the target exists, a file has unsaved changes, or the move fails.
  fn rename_project_folder(&mut self, path: &Path, name: &str, cx: &mut Context<Self>) -> Result<PathBuf, String> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
      return Err("the name cannot be used as a folder name".into());
    }
    let target = path.with_file_name(name);
    if target == path {
      return Ok(target);
    }
    if target.exists() {
      return Err(format!("{} already exists", display_path(&target)));
    }
    if self.editor_panel.read(cx).has_unsaved_under(path) {
      return Err("save or discard the open files first".into());
    }
    std::fs::rename(path, &target).map_err(|error| error.to_string())?;
    let active = self.projects.active.as_deref() == Some(path);
    self.editor_panel.update(cx, |editor, cx| editor.close_project(path, active, cx));
    self.projects.relocate(path, &target);
    Ok(target)
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
    self.close_project_search(cx);
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
    self.all_changes_open = false;
    self.all_changes.update(cx, |view, cx| view.reset(cx));
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
        // While the project menu is open, what each project holds is kept up to date; otherwise the favorites' tabs are, less often.
        if this.projects_menu_open {
          this.load_project_status(true, cx);
        } else if this.project_status_read.is_none_or(|read| read.elapsed() >= FAVORITE_STATUS_INTERVAL) {
          this.load_project_status(false, cx);
        }
        if this.all_changes_open
          && this.sidebar_tab == SidebarTab::Git
          && this.git_view == GitView::Changes
          && this.projects.active.as_ref() == Some(&root)
        {
          this.all_changes.update(cx, |view, cx| view.reload(root.clone(), cx));
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

  fn open_changed_file(&mut self, action: &OpenChangedFile, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let Some(path) = vekro_git::workdir_file(&root, Path::new(action.0.as_str())) else {
      window.push_notification(Notification::error("Could not locate the file in the working tree."), cx);
      return;
    };
    // Git may resolve the project directory through a symlink. Keep Explorer's path spelling.
    let path = if path.starts_with(&root) {
      path
    } else {
      root.canonicalize().ok().and_then(|canonical| path.strip_prefix(canonical).ok()).map(|relative| root.join(relative)).unwrap_or(path)
    };
    self.reveal_file(root, path, window, cx);
  }

  /// Opens a file in the editor and shows it in the Explorer: its folders open and the tree scrolls to it.
  fn reveal_file(&mut self, root: PathBuf, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    self.sidebar_tab = SidebarTab::Explorer;
    self.close_project_search(cx);
    let expanded = self.expanded.len();
    self.expanded.extend(path.ancestors().skip(1).take_while(|parent| *parent != root && parent.starts_with(&root)).map(Path::to_path_buf));
    if self.expanded.len() != expanded {
      self.rows = explorer::rows(&root, &self.expanded);
    }
    if let Some(index) = self.rows.iter().position(|row| row.path == path) {
      self.explorer_scroll.scroll_to_item(index, ScrollStrategy::Nearest);
    }
    self.open_file(path, window, cx);
  }

  pub fn search_project(&mut self, _: &SearchProject, window: &mut Window, cx: &mut Context<Self>) {
    self.sidebar_tab = SidebarTab::Search;
    if self.full_area_open() {
      self.hide_terminal(window, cx);
    }
    if !self.search_open {
      self.search_state = cx.new(|cx| InputState::new(window, cx).placeholder("Search project files"));
      self.search_include = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. src/**, *.rs"));
      self.search_exclude = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. tests/**"));
      self.search_replace = cx.new(|cx| InputState::new(window, cx).placeholder("Replace with"));
      self.search_subscriptions = [&self.search_state, &self.search_include, &self.search_exclude]
        .into_iter()
        .map(|input| {
          cx.subscribe_in(input, window, |this, input, event, window, cx| match event {
            InputEvent::Change => this.run_project_search(window, cx),
            InputEvent::PressEnter { .. } if *input == this.search_state => {
              if let Some(found) = this.search_results.iter().find(|found| !this.search_collapsed.contains(&found.path)).cloned() {
                this.open_search_result(&found, window, cx);
              }
            }
            _ => {}
          })
        })
        .collect();
      self.search_open = true;
    }
    if self.projects.active.is_some() {
      self.search_state.read(cx).focus_handle(cx).focus(window, cx);
    }
    cx.notify();
  }

  fn run_project_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.search_epoch += 1;
    let epoch = self.search_epoch;
    self.search_task = None;
    self.search_cancellation = None;
    self.search_results.clear();
    self.search_scroll.scroll_to_item(0, ScrollStrategy::Top);
    self.search_collapsed.clear();
    self.search_occurrences = 0;
    self.search_files = 0;
    self.search_error = None;
    self.search_options.query = self.search_state.read(cx).value().to_string();
    self.search_options.include = self.search_include.read(cx).value().to_string();
    self.search_options.exclude = self.search_exclude.read(cx).value().to_string();
    let Some(root) = self.projects.active.clone().filter(|_| !self.search_options.query.is_empty()) else {
      cx.notify();
      return;
    };
    let options = self.search_options.clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    self.search_cancellation = Some(SearchCancellation(cancelled.clone()));
    self.search_task = Some(cx.spawn_in(window, async move |this, cx| {
      cx.background_executor().timer(Duration::from_millis(200)).await;
      let work = cx.background_executor().spawn(async move { project_search::search(&root, &options, &cancelled) });
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.search_epoch != epoch {
          return;
        }
        this.search_cancellation = None;
        match result {
          Ok(results) => {
            this.search_occurrences = results.occurrences;
            this.search_files = results.files;
            this.search_results = results.matches;
          }
          Err(message) => this.search_error = Some(message.into()),
        }
        cx.notify();
      });
    }));
    cx.notify();
  }

  fn open_search_results(&mut self, _: &OpenSearchResults, window: &mut Window, cx: &mut Context<Self>) {
    if self.sidebar_tab != SidebarTab::Search || self.search_results.is_empty() {
      return;
    }
    use std::fmt::Write as _;
    let mut text = format!("{}\n", self.search_status());
    let root = self.projects.active.as_deref().unwrap_or_else(|| Path::new(""));
    for group in self.search_results.chunk_by(|left, right| left.path == right.path) {
      let path = &group[0].path;
      let path = path.strip_prefix(root).unwrap_or(path);
      let _ = writeln!(text, "\n{}", path.display());
      for found in group {
        let _ = writeln!(text, "  {}:{}  {}", found.line + 1, found.column + 1, found.preview);
      }
    }
    self.editor_panel.update(cx, |editor, cx| editor.show_search_report(&self.search_options.query, text, window, cx));
  }

  fn prepare_search_replacement(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone().filter(|_| self.search_occurrences > 0 && self.replacement_task.is_none()) else { return };
    let options = self.search_options.clone();
    let replacement = self.search_replace.read(cx).value().to_string();
    let preserve_case = self.search_preserve_case;
    let epoch = self.search_epoch;
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move { project_search::replacement_plan(&root, &options, &replacement, preserve_case) }
    });
    self.replacement_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.replacement_task = None;
        cx.notify();
        if this.projects.active.as_ref() != Some(&root) || this.search_epoch != epoch {
          return;
        }
        let edits = match result {
          Ok(edits) if !edits.is_empty() => Arc::new(edits),
          Ok(_) => {
            window.push_notification(Notification::info("The replacement would leave the files unchanged."), cx);
            return;
          }
          Err(message) => {
            window.push_notification(Notification::error(message), cx);
            return;
          }
        };
        let title: SharedString = format!("Replace matches in {} file{}?", edits.len(), if edits.len() == 1 { "" } else { "s" }).into();
        let workspace = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
          let (workspace, root, edits) = (workspace.clone(), root.clone(), edits.clone());
          alert.title(title.clone()).description("This writes the replacement to the project files on disk.").confirm().ok_text("Replace all").on_ok(
            move |_, window, cx| {
              let _ = workspace.update(cx, |this, cx| this.apply_search_replacement(root.clone(), edits.clone(), window, cx));
              true
            },
          )
        });
      });
    }));
    cx.notify();
  }

  fn apply_search_replacement(&mut self, root: PathBuf, edits: Arc<Vec<project_search::Replacement>>, window: &mut Window, cx: &mut Context<Self>) {
    if self.projects.active.as_ref() != Some(&root) || self.replacement_task.is_some() {
      return;
    }
    if edits.iter().any(|edit| self.editor_panel.read(cx).is_dirty(&edit.path)) {
      window.push_notification(Notification::error("Save the affected editor tabs and search again before replacing."), cx);
      return;
    }
    self.editor_panel.update(cx, |editor, cx| {
      editor.lock_replacement_files(edits.iter().map(|edit| edit.path.clone()), cx);
    });
    let work = cx.background_executor().spawn({
      let root = root.clone();
      let edits = edits.clone();
      async move { edits.iter().map(|edit| project_search::apply_replacement(&root, edit)).collect::<Vec<_>>() }
    });
    self.replacement_task = Some(cx.spawn_in(window, async move |this, cx| {
      let results = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.replacement_task = None;
        let mut written = 0;
        let mut first_error = None;
        this.editor_panel.update(cx, |editor, cx| {
          editor.lock_replacement_files([], cx);
          for (edit, result) in edits.iter().zip(results) {
            match result {
              Ok(()) => {
                written += 1;
                editor.update_replaced_text(&edit.path, &edit.updated, window, cx);
              }
              Err(error) => {
                first_error.get_or_insert_with(|| format!("{}: {error}", edit.path.display()));
              }
            }
          }
        });
        if let Some(error) = first_error {
          window.push_notification(Notification::error(format!("Updated {written} of {} files. {error}", edits.len())), cx);
        } else {
          window.push_notification(Notification::info(format!("Updated {written} file{}.", if written == 1 { "" } else { "s" })), cx);
        }
        if this.projects.active.as_ref() == Some(&root) && this.search_open {
          this.run_project_search(window, cx);
        }
        cx.notify();
      });
    }));
    cx.notify();
  }

  fn toggle_all_changes(&mut self, _: &ToggleAllChanges, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    self.all_changes_open = !self.all_changes_open;
    if self.all_changes_open {
      self.hide_terminal(window, cx);
      self.all_changes.update(cx, |view, cx| view.reload(root, cx));
    }
    cx.notify();
  }

  /// Shows the changes of a changed file in the main area, in place of the editor. Choosing another file replaces them.
  fn open_change(&mut self, file: PathBuf, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    self.all_changes_open = false;
    self.change_view.update(cx, |view, cx| view.show(root, file.clone(), cx));
    self.selected_change = Some(file);
    self.selected_entry = None;
    cx.notify();
  }

  /// Opens a changed file itself, not its diff, in the editor.
  pub(crate) fn open_change_in_editor(&mut self, file: &Path, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    match vekro_git::workdir_file(&root, file).filter(|path| path.is_file()) {
      Some(path) => self.reveal_file(root, path, window, cx),
      None => window.push_notification(Notification::error(format!("{} is not on the disk.", file.display())), cx),
    }
  }

  fn show_change_in_finder(root: PathBuf, file: PathBuf, window: &mut Window, cx: &mut App) {
    let work = cx.background_executor().spawn(async move {
      let path = vekro_git::workdir_file(&root, &file).ok_or("The repository's working folder is unavailable.")?;
      let mut command = std::process::Command::new("/usr/bin/open");
      if path.exists() {
        command.arg("-R").arg(path);
      } else {
        let folder = path.parent().and_then(|parent| parent.ancestors().find(|folder| folder.is_dir()));
        command.arg(folder.ok_or("No containing folder is available.")?);
      }
      let result = command.output().map_err(|error| error.to_string())?;
      if result.status.success() {
        return Ok(());
      }
      let error = String::from_utf8_lossy(&result.stderr);
      Err(if error.trim().is_empty() { result.status.to_string() } else { error.trim().to_owned() })
    });
    window
      .spawn(cx, async move |cx| {
        if let Err(error) = work.await {
          let _ = cx.update(|window, cx| window.push_notification(Notification::error(format!("Could not show the file in Finder. {error}")), cx));
        }
      })
      .detach();
  }

  /// Confirms the exact set of files before starting an irreversible discard.
  fn confirm_discard_changes(&self, files: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    if files.is_empty() || self.committing || self.discarding {
      return;
    }
    let count = files.len();
    let (title, description) = if let [file] = files.as_slice() {
      (
        format!("Discard changes to {}?", file.display()),
        "Staged and unstaged changes will be lost. New files will be deleted. This cannot be undone.".to_owned(),
      )
    } else {
      let mut names = files.iter().take(5).map(|file| file.display().to_string()).collect::<Vec<_>>().join("\n");
      if count > 5 {
        names.push_str(&format!("\n… and {} more", count - 5));
      }
      (
        format!("Discard changes to {count} files?"),
        format!("Staged and unstaged changes will be lost. New files will be deleted. This cannot be undone.\n\n{names}"),
      )
    };
    let workspace = cx.entity();
    let (title, description) = (SharedString::from(title), SharedString::from(description));
    window.open_alert_dialog(cx, move |alert, _, _| {
      let (workspace, root, files) = (workspace.clone(), root.clone(), files.clone());
      alert
        .title(title.clone())
        .description(description.clone())
        .width(px(430.))
        .confirm()
        .ok_text("Discard")
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, window, cx| {
          workspace.update(cx, |this, cx| this.discard_changes(root.clone(), files.clone(), window, cx));
          true
        })
    });
  }

  fn discard_changes(&mut self, root: PathBuf, files: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
    if self.committing || self.discarding || self.projects.active.as_ref() != Some(&root) {
      return;
    }
    self.discarding = true;
    let work_root = root.clone();
    let work = cx.background_executor().spawn(async move {
      let mut discarded = Vec::new();
      let mut failures = Vec::new();
      for file in files {
        match vekro_git::discard_changes(&work_root, &file) {
          Ok(()) => discarded.push((vekro_git::workdir_file(&work_root, &file), file)),
          Err(error) => failures.push(format!("{}: {error}", file.display())),
        }
      }
      (discarded, failures)
    });
    cx.spawn_in(window, async move |this, cx| {
      let (discarded, failures) = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.discarding = false;
        let active = this.projects.active.as_ref() == Some(&root);
        for (path, file) in &discarded {
          if let Some(path) = path {
            this.editor_panel.update(cx, |editor, cx| editor.forget(path, cx));
          }
          if active {
            this.excluded.remove(file);
            if this.selected_change.as_ref() == Some(file) {
              this.selected_change = None;
            }
          }
        }
        if active {
          this.refresh_in_background(cx);
        }
        let count = discarded.len();
        if let Some(error) = failures.first() {
          window.push_notification(Notification::error(format!("Discarded {count} of {} files. {error}", count + failures.len())), cx);
        } else {
          let message = if count == 1 { "Discarded changes to 1 file.".to_owned() } else { format!("Discarded changes to {count} files.") };
          window.push_notification(Notification::info(message), cx);
        }
        cx.notify();
      });
    })
    .detach();
    cx.notify();
  }

  /// Commits the changed files that are ticked, with the summary and description typed below the list.
  fn commit_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (Some(root), Some(git)) = (self.projects.active.clone(), self.git.as_ref()) else { return };
    if self.committing || self.discarding || git.merging || self.merge_task.is_some() {
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
    if self.remote_work.is_some() || self.merge_task.is_some() || self.git.as_ref().is_some_and(|git| git.merging) {
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
    let fetching_after_push = Arc::new(AtomicBool::new(false));
    let fraction = Arc::new(AtomicU32::new(0));
    self.remote_work = Some(RemoteWork {
      title: title.into(),
      progress: progress.clone(),
      fraction: fraction.clone(),
      started: std::time::Instant::now(),
      fetching_after_push: fetching_after_push.clone(),
    });
    cx.notify();
    let accounts = self.settings.read(cx).accounts.clone();
    let work = cx.background_executor().spawn({
      let root = root.clone();
      // `start` and `span` place the step in the whole run: a push is followed by a fetch.
      let report = move |start: f32, span: f32, line: &str| {
        *progress.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = line.to_string();
        let done = start + span * progress_fraction(line).unwrap_or(0.);
        fraction.fetch_max((done * 1000.) as u32, Ordering::Relaxed);
      };
      async move {
        let credentials = credentials_for_repository(&accounts, &root);
        if sync.behind > 0 {
          vekro_git::pull(&root, credentials.as_ref(), |line| report(0., 1., line))
        } else if sync.ahead > 0 || !sync.upstream {
          let pushed = vekro_git::push(&root, credentials.as_ref(), |line| report(0., 0.85, line))?;
          report(0.85, 0.15, "Connecting…");
          fetching_after_push.store(true, Ordering::Relaxed);
          vekro_git::fetch(&root, credentials.as_ref(), |line| report(0.85, 0.15, line))
            .map_err(|error| format!("Push succeeded, but fetching failed: {error}"))?;
          Ok(pushed)
        } else {
          vekro_git::fetch(&root, credentials.as_ref(), |line| report(0., 1., line))
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
        if let Some(work) = this.remote_work.take() {
          // The fill runs on to the right edge and goes away.
          this.remote_runs += 1;
          let run = this.remote_runs;
          this.remote_finish = Some((work.shown(), run));
          cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REMOTE_FINISH * 3 / 2).await;
            let _ = this.update(cx, |this, cx| {
              if this.remote_finish.is_some_and(|(_, current)| current == run) {
                this.remote_finish = None;
                cx.notify();
              }
            });
          })
          .detach();
        }
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

  /// Starts the explicitly requested merge or reopens its conflict resolver.
  fn merge_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (Some(root), Some(git)) = (self.projects.active.clone(), self.git.as_ref()) else { return };
    if self.merge_task.is_some() || self.remote_work.is_some() || self.committing || self.discarding {
      return;
    }
    let pending = git.merging;
    let branch = self.compare.as_ref().map(|view| view.branch.clone());
    if !pending && branch.is_none() {
      return;
    }
    let work = cx.background_executor().spawn({
      let root = root.clone();
      let branch = branch.clone();
      let current = git.branch.clone();
      async move {
        if pending {
          vekro_git::read_merge(&root).map(vekro_git::MergeOutcome::Conflicted)
        } else {
          vekro_git::start_merge(&root, branch.as_deref().unwrap_or_default(), &current)
        }
      }
    });
    self.merge_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.merge_task = None;
        if this.projects.active.as_ref() != Some(&root) {
          cx.notify();
          return;
        }
        this.refresh_project();
        match result {
          Ok(vekro_git::MergeOutcome::Finished) => this.merge_completed(branch.as_deref(), window, cx),
          Ok(vekro_git::MergeOutcome::Conflicted(state)) => {
            if let Some((_, dialog, _)) = this.merge_dialog.as_ref().filter(|(path, dialog, _)| path == &root && dialog.read(cx).same_merge(&state)) {
              dialog.update(cx, |dialog, cx| dialog.set_state(state, cx));
              Self::show_merge_dialog(dialog.clone(), window, cx);
              cx.notify();
              return;
            }
            let source = if pending { "incoming changes".into() } else { branch.unwrap_or_default().into() };
            let dialog = cx.new(|_| MergeDialog::new(root.clone(), source, state));
            let subscription = cx.subscribe_in(&dialog, window, |this, dialog, event, window, cx| {
              let source = dialog.read(cx).source().clone();
              window.close_dialog(cx);
              this.merge_dialog = None;
              this.refresh_project();
              match event {
                MergeEvent::Completed => this.merge_completed(Some(source.as_str()), window, cx),
                MergeEvent::Aborted => window.push_notification(Notification::info("Merge aborted."), cx),
              }
              cx.notify();
            });
            this.merge_dialog = Some((root, dialog.clone(), subscription));
            Self::show_merge_dialog(dialog, window, cx);
          }
          Err(message) => window.push_notification(Notification::error(message), cx),
        }
        cx.notify();
      });
    }));
    cx.notify();
  }

  fn show_merge_dialog(dialog: Entity<MergeDialog>, window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |surface, window, _| {
      surface
        .title("Resolve merge conflicts")
        .width(window.rem_size() * 42.)
        .close_button(false)
        .overlay_closable(false)
        .keyboard(false)
        .child(dialog.clone())
    });
  }

  fn merge_completed(&mut self, source: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
    let target = self.git.as_ref().map_or("the current branch", |git| git.branch.as_str());
    let source = source.unwrap_or("incoming changes");
    window.push_notification(Notification::info(format!("Merged {source} into {target}.")), cx);
    self.clear_compare(cx);
    self.selected_change = None;
    self.all_changes_open = false;
    self.set_git_view(GitView::Changes, cx);
    self.load_history(cx);
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
      self.load_project_status(true, cx);
      self.project_filter.update(cx, |input, cx| input.set_value("", window, cx));
      let focus = self.project_filter.read(cx).focus_handle(cx);
      window.defer(cx, move |window, cx| focus.focus(window, cx));
    }
    cx.notify();
  }

  /// What a project holds: for the open project what the window already read, for the others what the last background read found.
  fn status_of(&self, path: &Path) -> Option<ProjectStatus> {
    if self.projects.active.as_deref() == Some(path) {
      if let Some(git) = &self.git {
        let (changed, push) = git.status();
        return Some(ProjectStatus { changed, push });
      }
    }
    self.project_status.get(path).copied()
  }

  /// Reads what projects hold (changed files, commits to push) off the interface thread: every project for the project menu
  /// (`all`), else the favorites, whose tabs show it.
  fn load_project_status(&mut self, all: bool, cx: &mut Context<Self>) {
    self.project_status_read = Some(Instant::now());
    let paths: Vec<PathBuf> = if all {
      self.projects.projects.iter().map(|project| project.path.clone()).collect()
    } else {
      self.projects.favorites().map(|project| project.path.clone()).collect()
    };
    let work = cx.background_executor().spawn(async move {
      paths
        .into_iter()
        .filter_map(|path| vekro_git::quick_status(&path).map(|(changed, push)| (path, ProjectStatus { changed, push })))
        .collect::<HashMap<_, _>>()
    });
    cx.spawn(async move |this, cx| {
      let found = work.await;
      let _ = this.update(cx, |this, cx| {
        // All projects replace what was known; the favorites only refresh their own entries.
        let mut next = if all { HashMap::new() } else { this.project_status.clone() };
        next.extend(found);
        if this.project_status != next {
          this.project_status = next;
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
      .custom(
        ButtonCustomVariant::new(cx).color(palette.rail).foreground(palette.text).hover(palette.rail_item_hover).active(palette.rail_item_hover),
      )
      .cursor_pointer()
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
            .children([Tab::new().cursor_pointer().label("Branches"), Tab::new().cursor_pointer().label(format!("Pull Requests{count}"))])
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

  /// The title bar's project menu, grouped by category and filtered by typing, with a way to add a project.
  fn render_repository_switcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let workspace = cx.entity();
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let trigger = Button::new("repository-trigger")
      .custom(ButtonCustomVariant::new(cx).color(palette.rail).foreground(palette.text).hover(palette.rail_item_hover).active(palette.selected))
      .cursor_pointer()
      .border_0()
      .h_8()
      .max_w_56()
      .rounded_md()
      .tooltip("Projects")
      .child(h_flex().gap_2().items_center().child(Icon::empty().path("icons/folder.svg").size_5()).child(
        Icon::new(if self.projects_menu_open { IconName::ChevronUp } else { IconName::ChevronDown }).size_4().text_color(theme.muted_foreground),
      ));
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

  fn render_favorite_projects(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
    let palette = colors(cx);
    let workspace = cx.entity();
    let primary = cx.theme().primary;
    self
      .projects
      .favorites()
      .map(|project| {
        let path = project.path.clone();
        let selected = self.projects.active.as_ref() == Some(&path);
        let look = avatar_look(project, selected, cx);
        let status = self.status_of(&path);
        let workspace = workspace.clone();
        Button::new(SharedString::from(format!("favorite-project-{}", path.display())))
          .custom(
            ButtonCustomVariant::new(cx)
              .color(if selected { primary } else { palette.rail })
              .foreground(if selected { palette.on_accent } else { palette.text })
              .hover(if selected { primary } else { palette.rail_item_hover })
              .active(if selected { primary } else { palette.rail_item_hover }),
          )
          // The kit paints a custom color at 20% at rest and in full only on hover: the open project is filled in full.
          .when(selected, |button| button.bg(primary))
          .cursor_pointer()
          .border_0()
          .h_8()
          .max_w_56()
          .rounded_md()
          .child(
            h_flex()
              .gap_2()
              .items_center()
              .when_some(look.icon.clone(), |row, icon| {
                row.child(
                  div()
                    .size_6()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .text_color(if selected { palette.on_accent } else { look.text })
                    .when_some(look.fill.filter(|_| !selected), |avatar, fill| avatar.bg(fill))
                    .child(Icon::empty().path(icon).size_4()),
                )
              })
              .child(
                Label::new(project.name())
                  .when(selected, |label| label.text_color(palette.on_accent))
                  .min_w_0()
                  .font_weight(FontWeight::SEMIBOLD)
                  .overflow_hidden()
                  .whitespace_nowrap()
                  .text_ellipsis(),
              )
              .children(project_status_marks("favorite", &path, status.as_ref(), selected, cx)),
          )
          .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.select_project(path.clone(), window, cx)))
          .into_any_element()
      })
      .collect()
  }

  /// The bar above the editor in the Git tab: the branch with its menu, and the one button that keeps the branch in step with
  /// its remote: publish, push, pull or fetch, whichever the branch needs.
  fn render_git_toolbar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    let git = self.git.as_ref()?;
    let theme = cx.theme().clone();
    let accent = theme.primary;
    let palette = colors(cx);
    let picker = self.render_branch_picker(&git.branch, cx);
    let pending_merge = git.merging.then(|| {
      Button::new("reopen-merge")
        .small()
        .label("Resolve merge")
        .icon(Icon::empty().path("icons/triangle-alert.svg"))
        .disabled(self.merge_task.is_some())
        .on_click(cx.listener(|this, _, window, cx| this.merge_compare(window, cx)))
    });
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
        .custom(
          ButtonCustomVariant::new(cx).color(palette.rail).foreground(palette.text).hover(palette.rail_item_hover).active(palette.rail_item_hover),
        )
        .cursor_pointer()
        .border_0()
        .h_12()
        .w_56()
        .rounded_none()
        .disabled(self.remote_work.is_some() || git.merging || self.merge_task.is_some())
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
                .child(
                  Label::new(self.remote_work.as_ref().map_or(title.into(), |work| {
                    if work.fetching_after_push.load(Ordering::Relaxed) {
                      format!("Fetching {}", sync.remote).into()
                    } else {
                      work.title.clone()
                    }
                  }))
                  .font_weight(FontWeight::SEMIBOLD),
                )
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
        .bg(palette.rail)
        .border_b_1()
        .border_color(palette.border)
        .child(div().h_12().border_r_1().border_color(palette.border).child(picker))
        .children(sync.map(|button| {
          // While a push, pull or fetch runs, a soft fill of the accent color grows across the button from left to right as
          // far as git has got.
          let fill = div().absolute().top_0().bottom_0().left_0().bg(accent.opacity(0.15));
          let fill = if let Some(work) = &self.remote_work {
            Some(fill.w(relative(work.shown())).into_any_element())
          } else {
            self.remote_finish.map(|(from, run)| {
              fill
                .with_animation(("remote-finish", run), Animation::new(REMOTE_FINISH), move |fill, delta| {
                  fill.w(relative(from + (1. - from) * delta))
                })
                .into_any_element()
            })
          };
          div().relative().h_12().overflow_hidden().border_r_1().border_color(palette.border).child(button).children(fill)
        }))
        .children(pending_merge),
    )
  }

  fn render_git(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let git = self.git.as_ref()?;
    let root = self.projects.active.as_ref()?;
    let changed = match git.changed() {
      0 => "No changes".to_string(),
      1 => "1 changed file".to_string(),
      count => format!("{count} changed files"),
    };
    let chosen = git.changes.iter().filter(|change| !self.excluded.contains(&change.path)).count();
    let workspace = cx.entity();
    let changes_list =
      v_flex().id("git-changes").flex_1().min_h_0().overflow_y_scrollbar().children(git.changes.iter().map(|change| {
        let file = change.path.clone();
        let selected = !self.all_changes_open && self.selected_change.as_ref() == Some(&change.path);
        let included = !self.excluded.contains(&change.path);
        let name = change.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let folder = change.path.parent().map(|parent| parent.display().to_string()).filter(|parent| !parent.is_empty());
        h_flex()
          .id(SharedString::from(format!("change-row-{}", change.path.display())))
          .gap_1p5()
          .px_3()
          .py_1()
          .border_b_1()
          .border_color(palette.border)
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
              .child(
                h_flex().flex_1().min_w_0().gap_2().child(Label::new(name).min_w_0().font_weight(FontWeight::MEDIUM).truncate()).when_some(
                  folder,
                  |row, folder| {
                    row.child(Label::new(folder).flex_1().min_w_0().text_size(FONT_SIZE_SM).text_color(theme.muted_foreground).truncate())
                  },
                ),
              )
              .child(Icon::empty().path(change_kind_icon(change.kind)).size_4().flex_none().text_color(kind_color(change.kind, &palette)))
              .on_click(cx.listener({
                let file = file.clone();
                // A double click opens the file itself, in the editor with the Explorer; one click shows its diff.
                move |this, event: &ClickEvent, window, cx| {
                  if event.click_count() >= 2 {
                    this.open_change_in_editor(&file, window, cx);
                  } else {
                    this.open_change(file.clone(), cx);
                  }
                }
              })),
          )
          .context_menu({
            let (workspace, deleted) = (workspace.clone(), change.kind == vekro_git::ChangeKind::Deleted);
            let root = root.clone();
            let busy = self.committing || self.discarding;
            move |menu, _, _| {
              let (open_workspace, open_file) = (workspace.clone(), file.clone());
              let (discard_workspace, discard_file) = (workspace.clone(), file.clone());
              let (finder_root, finder_file) = (root.clone(), file.clone());
              let copy_path = file.to_string_lossy().into_owned();
              menu
                .item(
                  PopupMenuItem::new("Open in Editor")
                    .icon(Icon::empty().path("icons/file-pen-line.svg"))
                    .disabled(deleted)
                    .on_click(move |_, window, cx| open_workspace.update(cx, |this, cx| this.open_change_in_editor(&open_file, window, cx))),
                )
                .item(
                  PopupMenuItem::new(if deleted { "Open Containing Folder" } else { "Show in Finder" })
                    .icon(Icon::empty().path("icons/folder-open.svg"))
                    .on_click(move |_, window, cx| Self::show_change_in_finder(finder_root.clone(), finder_file.clone(), window, cx)),
                )
                .item(
                  PopupMenuItem::new("Copy Relative Path")
                    .icon(Icon::empty().path("icons/copy.svg"))
                    .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy_path.clone()))),
                )
                .separator()
                .item(PopupMenuItem::new("Discard Changes…").icon(Icon::empty().path("icons/trash.svg")).disabled(busy).on_click(
                  move |_, window, cx| discard_workspace.update(cx, |this, cx| this.confirm_discard_changes(vec![discard_file.clone()], window, cx)),
                ))
            }
          })
      }));
    let summary_filled = !self.commit_summary.read(cx).value().trim().is_empty();
    let label = match chosen {
      1 => format!("Commit 1 file to {}", git.branch),
      count => format!("Commit {count} files to {}", git.branch),
    };
    let commit_box = v_flex()
      .flex_none()
      .gap_1p5()
      .px_3()
      .py_2()
      .border_t_1()
      .border_color(palette.border)
      .bg(palette.surface)
      .child(Input::new(&self.commit_summary).small().bg(palette.background).border_color(palette.border))
      .child(
        div().relative().child(Textarea::new(&self.commit_description).h(rems(4.5)).bg(palette.background).border_color(palette.border)).child(
          h_flex()
            .absolute()
            .right_1()
            .bottom_1()
            .gap_0p5()
            .child(
              Button::new("commit-settings")
                .custom(ButtonCustomVariant::new(cx).color(palette.surface).foreground(palette.text).hover(palette.hover).active(palette.hover))
                .cursor_pointer()
                .small()
                .icon(Icon::empty().path("icons/settings-2.svg"))
                .tooltip("OpenCommit settings")
                .on_click(cx.listener(|this, _, window, cx| this.open_commit_settings(window, cx))),
            )
            .child(
              Button::new("generate-message")
                .custom(ButtonCustomVariant::new(cx).color(palette.surface).foreground(cx.theme().primary).hover(palette.hover).active(palette.hover))
                .cursor_pointer()
                .small()
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
          .small()
          .w_full()
          .label(label)
          .disabled(chosen == 0 || !summary_filled || self.committing || self.discarding || git.merging || self.merge_task.is_some())
          .on_click(cx.listener(|this, _, window, cx| this.commit_changes(window, cx))),
      );
    let tabs = TabBar::new("git-view-tabs")
      .underline()
      .selected_index(usize::from(self.git_view == GitView::History))
      .on_click(cx.listener(|this, index: &usize, _, cx| this.set_git_view(if *index == 1 { GitView::History } else { GitView::Changes }, cx)))
      .children([
        Tab::new().cursor_pointer().label("Changes").when(git.changed() > 0, |tab| tab.suffix(count_badge(git.changed(), cx))),
        Tab::new().cursor_pointer().label("History"),
      ]);
    let body = match self.git_view {
      GitView::Changes => v_flex()
        .flex_1()
        .min_h_0()
        .child(
          h_flex()
            .id("changes-header")
            .justify_between()
            .px_3()
            .py_1()
            .bg(palette.rail)
            .border_b_1()
            .border_color(palette.border)
            .child(
              Checkbox::new("include-all-changes")
                .label(changed)
                .checked(git.changed() > 0 && chosen == git.changed())
                .indeterminate(chosen > 0 && chosen < git.changed())
                .disabled(git.changed() == 0)
                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                  if let Some(git) = &this.git {
                    for change in &git.changes {
                      if *checked {
                        this.excluded.remove(&change.path);
                      } else {
                        this.excluded.insert(change.path.clone());
                      }
                    }
                    cx.notify();
                  }
                })),
            )
            .child(
              Button::new("toggle-all-changes")
                .ghost()
                .xsmall()
                .selected(self.all_changes_open)
                .disabled(git.changes.is_empty() && !self.all_changes_open)
                .icon(Icon::empty().path("icons/layout-list.svg").size_4())
                .accessibility_label(if self.all_changes_open { "Close all changes" } else { "Show all changes" })
                .tooltip(if self.all_changes_open { "Close all changes" } else { "Show all changes" })
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleAllChanges), cx)),
            )
            .context_menu({
              let disabled = git.changes.is_empty() || self.committing || self.discarding;
              move |menu, _, _| {
                let workspace = workspace.clone();
                menu.item(PopupMenuItem::new("Discard All Changes…").icon(Icon::empty().path("icons/trash.svg")).disabled(disabled).on_click(
                  move |_, window, cx| {
                    workspace.update(cx, |this, cx| {
                      if let Some(git) = &this.git {
                        let files = git.changes.iter().map(|change| change.path.clone()).collect();
                        this.confirm_discard_changes(files, window, cx);
                      }
                    });
                  },
                ))
              }
            }),
        )
        .child(div().flex_1().min_h_0().when(git.changed() > 0, |list| list.child(changes_list)))
        .child(commit_box)
        .into_any_element(),
      GitView::History => v_flex().flex_1().min_h_0().pt_2().child(self.render_history(cx)).into_any_element(),
    };
    Some(v_flex().size_full().child(div().px_3().bg(palette.rail).child(tabs)).child(body))
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
    // The padding on the right is inside each list row, so that the scrollbar sits at the edge of the sidebar.
    v_flex().flex_1().min_h_0().gap_2().child(div().px_3().child(self.render_compare_field(cx))).child(v_flex().flex_1().min_h_0().pl_3().child(body))
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
      .child(div().absolute().inset_0().child(
        Scrollbar::new(&self.history_scroll).id("history-scrollbar").axis(ScrollbarAxis::Vertical).mode(ScrollbarMode::Always).viewport_from_layout(),
      ))
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
      .children([Tab::new().cursor_pointer().label(format!("Behind ({behind})")), Tab::new().cursor_pointer().label(format!("Ahead ({ahead})"))]);
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
    let footer = view.found.as_ref().map(|found| {
      let conflicts = found.conflict_count.unwrap_or_default();
      let message = if conflicts > 0 {
        note(vec![
          bold(format!("{conflicts} conflicted file{}", if conflicts == 1 { "" } else { "s" })).into_any_element(),
          Label::new(format!("when merging {} into {current}", view.branch)).into_any_element(),
        ])
      } else if behind > 0 {
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
      v_flex()
        .gap_2()
        .pr_3()
        .pb_3()
        .pt_2()
        .items_center()
        .child(if conflicts > 0 {
          Icon::empty().path("icons/triangle-alert.svg").size_4().text_color(palette.conflicted)
        } else {
          Icon::new(IconName::Check).size_4().text_color(palette.new)
        })
        .child(message)
        .when(behind > 0, |footer| {
          footer.child(
            div().w_full().child(
              Button::new("compare-merge")
                .primary()
                .w_full()
                .label(if self.git.as_ref().is_some_and(|git| git.merging) { "Resolve merge conflicts" } else { "Create a merge commit" })
                .loading(self.merge_task.is_some())
                .disabled(self.merge_task.is_some() || self.remote_work.is_some())
                .on_click(cx.listener(|this, _, window, cx| this.merge_compare(window, cx))),
            ),
          )
        })
    });
    v_flex().flex_1().min_h_0().gap_2().child(div().pr_3().child(tabs)).child(list).children(footer).into_any_element()
  }

  /// One line of the folder tree.
  fn explorer_row(&self, row: &explorer::Row, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let path = row.path.clone();
    let selected = self.selected_entry.as_ref() == Some(&row.path);
    let icon_color = if selected { palette.on_accent } else { theme.muted_foreground };
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
      .when(selected, |entry| entry.bg(theme.primary))
      .when(!selected, |entry| entry.hover(|entry| entry.bg(palette.hover)))
      .child(div().size_4().flex_none().when(row.is_dir, |chevron| {
        chevron.child(Icon::new(if open { IconName::ChevronDown } else { IconName::ChevronRight }).size_4().text_color(icon_color))
      }))
      // Only folders get an icon; a file's name starts where a folder's icon does.
      .when(row.is_dir, |entry| entry.child(Icon::new(IconName::FolderClosed).size_4().text_color(icon_color)))
      .child(
        Label::new(row.name.clone())
          .when(selected, |label| label.text_color(palette.on_accent))
          .flex_1()
          .min_w_0()
          .overflow_hidden()
          .whitespace_nowrap()
          .text_ellipsis(),
      )
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
    v_flex().flex_1().min_h_0().gap_2().child(header).child(rows).into_any_element()
  }

  fn close_project_search(&mut self, cx: &mut Context<Self>) {
    self.search_open = false;
    self.search_subscriptions.clear();
    self.search_options = project_search::Options::default();
    self.search_occurrences = 0;
    self.search_files = 0;
    self.search_error = None;
    if self.sidebar_tab == SidebarTab::Search {
      self.sidebar_tab = SidebarTab::Explorer;
    }
    self.search_task = None;
    self.search_cancellation = None;
    self.search_results.clear();
    self.search_collapsed.clear();
    self.search_epoch += 1;
    cx.notify();
  }

  fn open_search_result(&mut self, result: &project_search::Match, window: &mut Window, cx: &mut Context<Self>) {
    self.open_file(result.path.clone(), window, cx);
    self.editor_panel.update(cx, |editor, cx| editor.reveal_position(&result.path, result.line, result.column, window, cx));
  }

  fn search_status(&self) -> String {
    let count = self.search_occurrences;
    let files = self.search_files;
    if self.projects.active.is_none() {
      "Open a project to search its files.".into()
    } else if self.search_cancellation.is_some() {
      "Searching…".into()
    } else if self.search_options.query.is_empty() {
      String::new()
    } else if count == 0 {
      "No matches found".into()
    } else {
      let result_word = if count == 1 { "match" } else { "matches" };
      let file_word = if files == 1 { "file" } else { "files" };
      let limit = if self.search_results.len() < count { format!(" · showing {}", self.search_results.len()) } else { String::new() };
      format!("{count} {result_word} in {files} {file_word}{limit}")
    }
  }

  fn render_project_search(&self, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let loading = self.search_cancellation.is_some();
    let status = self.search_error.as_ref().map_or_else(|| self.search_status(), ToString::to_string);
    let option = |label, title, selected| Button::new(title).ghost().xsmall().label(label).tooltip(title).selected(selected);
    let focus = self.focus_handle.clone();
    let flags = h_flex()
      .gap_0p5()
      .child(option("Aa", "Match case", self.search_options.case_sensitive).on_click(cx.listener(|this, _, window, cx| {
        this.search_options.case_sensitive = !this.search_options.case_sensitive;
        this.run_project_search(window, cx);
      })))
      .child(option("ab", "Match whole word", self.search_options.whole_word).on_click(cx.listener(|this, _, window, cx| {
        this.search_options.whole_word = !this.search_options.whole_word;
        this.run_project_search(window, cx);
      })))
      .child(option(".*", "Use regular expression", self.search_options.regex).on_click(cx.listener(|this, _, window, cx| {
        this.search_options.regex = !this.search_options.regex;
        this.run_project_search(window, cx);
      })));
    let action = |id, icon, tooltip| Button::new(id).ghost().xsmall().icon(Icon::empty().path(icon)).tooltip(tooltip);
    v_flex()
      .id("project-search")
      .key_context("ProjectSearch")
      .flex_1()
      .min_h_0()
      .gap_2()
      .child(
        h_flex().justify_between().gap_2().child(Label::new("Search").font_weight(FontWeight::SEMIBOLD)).child(
          h_flex()
            .gap_1()
            .child(
              action("search-refresh", "icons/refresh-cw.svg", "Refresh results")
                .on_click(cx.listener(|this, _, window, cx| this.run_project_search(window, cx))),
            )
            .child(action("search-collapse", "icons/list-chevrons-down-up.svg", "Collapse or expand results").on_click(cx.listener(
              |this, _, _, cx| {
                if this.search_collapsed.is_empty() {
                  this.search_collapsed.extend(this.search_results.iter().map(|found| found.path.clone()));
                } else {
                  this.search_collapsed.clear();
                }
                cx.notify();
              },
            ))),
        ),
      )
      .child(
        h_flex()
          .gap_1()
          .child(
            Button::new("toggle-search-replace")
              .ghost()
              .xsmall()
              .icon(if self.search_show_replace { IconName::ChevronDown } else { IconName::ChevronRight })
              .accessibility_label("Toggle replace")
              .tooltip("Toggle replace")
              .on_click(cx.listener(|this, _, _, cx| {
                this.search_show_replace = !this.search_show_replace;
                cx.notify();
              })),
          )
          .child(Input::new(&self.search_state).small().w_full().disabled(self.projects.active.is_none()).suffix(flags)),
      )
      .when(self.search_show_replace, |panel| {
        panel.child(
          h_flex()
            .gap_1()
            .child(Input::new(&self.search_replace).small().w_full().disabled(self.projects.active.is_none()).suffix(
              option("AB", "Preserve case", self.search_preserve_case).on_click(cx.listener(|this, _, _, cx| {
                this.search_preserve_case = !this.search_preserve_case;
                cx.notify();
              })),
            ))
            .child(
              action("search-replace-all", "icons/replace-all.svg", "Replace all matches")
                .accessibility_label("Replace all matches")
                .disabled(self.search_occurrences == 0 || loading || self.replacement_task.is_some())
                .loading(self.replacement_task.is_some())
                .on_click(cx.listener(|this, _, window, cx| this.prepare_search_replacement(window, cx))),
            ),
        )
      })
      .child(
        h_flex().justify_end().child(
          action("search-filters", "icons/list-filter.svg", "Toggle file filters")
            .selected(self.search_show_filters)
            .accessibility_label("Toggle file filters")
            .on_click(cx.listener(|this, _, _, cx| {
              this.search_show_filters = !this.search_show_filters;
              cx.notify();
            })),
        ),
      )
      .when(self.search_show_filters, |panel| {
        panel.child(
          v_flex()
            .gap_2()
            .child(v_flex().gap_1().child(Label::new("Files to include").text_xs()).child(Input::new(&self.search_include).small()))
            .child(v_flex().gap_1().child(Label::new("Files to exclude").text_xs()).child(Input::new(&self.search_exclude).small())),
        )
      })
      .when(!status.is_empty(), |panel| {
        panel.child(
          h_flex()
            .gap_2()
            .when(loading, |row| row.child(Spinner::new().small()))
            .flex_wrap()
            .child(Label::new(status).text_size(FONT_SIZE_SM).text_color(if self.search_error.is_some() {
              theme.danger
            } else {
              theme.muted_foreground
            }))
            .when(!self.search_results.is_empty(), |row| {
              row.child(
                Button::new("open-search-results")
                  .link()
                  .xsmall()
                  .label("Open in editor")
                  .on_click(move |_, window, cx| focus.dispatch_action(&OpenSearchResults, window, cx)),
              )
            }),
        )
      })
      .child(self.render_search_results(cx))
      .into_any_element()
  }

  fn render_search_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let mut rows = Vec::new();
    let mut first = 0;
    for group in self.search_results.chunk_by(|left, right| left.path == right.path) {
      rows.push(SearchListRow::File { first, count: group.len() });
      if !self.search_collapsed.contains(&group[0].path) {
        rows.extend((first..first + group.len()).map(SearchListRow::Match));
      }
      first += group.len();
    }
    let list = uniform_list(
      "project-search-results",
      rows.len(),
      cx.processor(move |this, range: Range<usize>, _, cx| {
        rows
          .get(range)
          .unwrap_or_default()
          .iter()
          .map(|row| match *row {
            SearchListRow::File { first, count } => this.render_search_file(first, count, cx),
            SearchListRow::Match(index) => this.render_search_match(index, cx),
          })
          .collect::<Vec<_>>()
      }),
    )
    .track_scroll(&self.search_scroll)
    .size_full();
    div()
      .relative()
      .flex_1()
      .min_h_0()
      .child(list)
      .child(div().absolute().inset_0().child(Scrollbar::new(&self.search_scroll).axis(ScrollbarAxis::Vertical).viewport_from_layout()))
  }

  fn render_search_file(&self, first: usize, count: usize, cx: &mut Context<Self>) -> AnyElement {
    let path = self.search_results[first].path.clone();
    let root = self.projects.active.as_deref().unwrap_or_else(|| Path::new(""));
    let relative = path.strip_prefix(root).unwrap_or(&path);
    let filename = SharedString::from(relative.file_name().unwrap_or_default().to_string_lossy().into_owned());
    let folder = SharedString::from(relative.parent().unwrap_or_else(|| Path::new("")).display().to_string());
    let collapsed = self.search_collapsed.contains(&path);
    let palette = colors(cx);
    h_flex()
      .id(SharedString::from(format!("search-file-{}", path.display())))
      .h_7()
      .gap_1()
      .pr_3()
      .cursor_pointer()
      .rounded_md()
      .hover(|row| row.bg(palette.hover))
      .child(Icon::new(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown }).xsmall())
      .child(Label::new(filename).font_weight(FontWeight::SEMIBOLD).truncate())
      .child(Label::new(folder).flex_1().min_w_0().text_xs().text_color(palette.text_secondary).truncate())
      .child(div().rounded_full().bg(palette.selected).px_2().text_xs().text_color(palette.text).child(count.to_string()))
      .on_click(cx.listener(move |this, _, _, cx| {
        if !this.search_collapsed.remove(&path) {
          this.search_collapsed.insert(path.clone());
        }
        cx.notify();
      }))
      .into_any_element()
  }

  fn render_search_match(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
    let found = self.search_results[index].clone();
    let palette = colors(cx);
    h_flex()
      .id(SharedString::from(format!("search-match-{}-{}-{}", found.path.display(), found.line, found.column)))
      .h_7()
      .gap_2()
      .pl_4()
      .pr_3()
      .cursor_pointer()
      .rounded_md()
      .hover(|row| row.bg(palette.hover))
      .child(Label::new((found.line + 1).to_string()).text_xs().text_color(palette.text_secondary))
      .child(div().min_w_0().truncate().child(StyledText::new(SharedString::from(found.preview.clone())).with_highlights([(
        found.highlight.clone(),
        HighlightStyle { background_color: Some(cx.theme().primary.opacity(0.28)), ..Default::default() },
      )])))
      .on_click(cx.listener(move |this, _, window, cx| this.open_search_result(&found, window, cx)))
      .into_any_element()
  }

  /// The repository summary and the folder tree.
  fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let content = match self.sidebar_tab {
      SidebarTab::Explorer => self.render_explorer(cx).into_any_element(),
      SidebarTab::Search => self.render_project_search(cx),
      SidebarTab::Git => match self.render_git(cx) {
        Some(git) => git.into_any_element(),
        None => Label::new("Not a Git repository").text_color(cx.theme().muted_foreground).p_3().into_any_element(),
      },
    };
    v_flex().size_full().bg(cx.theme().sidebar).child(
      v_flex()
        .flex_1()
        .min_h_0()
        .when(self.sidebar_tab == SidebarTab::Explorer, |sidebar| sidebar.pl_3().pt_3())
        .when(self.sidebar_tab == SidebarTab::Search, |sidebar| sidebar.px_3().pt_3())
        .child(content),
    )
  }

  /// The narrow bar at the far left: one button for each view of the sidebar (Explorer, Git), the open one marked; the terminal
  /// button under them; and project settings, with a small warning at its corner while an account needs attention.
  fn render_activity_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let workspace = cx.entity();
    let changed = self.git.as_ref().map_or(0, Summary::changed);
    let buttons = SidebarTab::ALL.map(|tab| {
      let workspace = workspace.clone();
      let focus = self.focus_handle.clone();
      let button =
        rail_button(tab.label(), tab.icon(), tab.label(), tab == self.sidebar_tab && !self.full_area_open(), true, &palette, move |_, window, cx| {
          if tab == SidebarTab::Search {
            focus.dispatch_action(&SearchProject, window, cx);
            return;
          }
          workspace.update(cx, |this, cx| {
            this.sidebar_tab = tab;
            // The view of the sidebar and the editor come back when the terminals or the AI view were taking the main area.
            if this.full_area_open() {
              this.hide_terminal(window, cx);
            }
            cx.notify();
          })
        });
      // The Git button shows how many files have changes, in a small blue circle at its top right.
      let badge = (tab == SidebarTab::Git && changed > 0).then(|| count_badge(changed, cx).absolute().top_0().right(-BADGE_OVERHANG));
      div().relative().child(button).children(badge)
    });
    let attention = self.settings.read(cx).accounts.items.iter().any(|account| account.issue.is_some());
    let terminal_attention = self.terminal_tabs.values().any(|tabs| tabs.panes().iter().any(|terminal| terminal.read(cx).needs_attention()));
    let terminal =
      rail_button("terminal-toggle", Icon::empty().path("icons/terminal.svg"), "Terminal", self.terminal_open, true, &palette, |_, window, cx| {
        window.dispatch_action(Box::new(ToggleTerminal), cx)
      });
    let agents = rail_button(
      "agents-toggle",
      Icon::empty().path("icons/bot-message-square.svg"),
      "AI agents",
      self.agent_open,
      true,
      &palette,
      |_, window, cx| window.dispatch_action(Box::new(ToggleAgents), cx),
    );
    let settings = div()
      .relative()
      .child(rail_button("project-settings-open", Icon::new(IconName::Settings), "Project settings", false, true, &palette, {
        let workspace = workspace.clone();
        move |_, window, cx| {
          workspace.update(cx, |this, cx| {
            if let Some(path) = this.projects.active.clone() {
              this.open_project_settings(&path, window, cx);
            }
          })
        }
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
      .child(agents)
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

  /// The AI view, filling the main area. It is not a terminal window: a header names the project and holds one tab per
  /// session (agent icon and name) and a menu to start another agent, and with no session it shows the choice of agents.
  fn render_agent_area(&self, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let Some(root) = self.projects.active.clone() else {
      return Label::new("No project is open.").text_color(theme.muted_foreground).p_3().into_any_element();
    };
    let project = SharedString::from(root.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default());
    let workspace = cx.entity();
    let picker =
      Button::new("agent-new").ghost().xsmall().icon(Icon::empty().path("icons/plus.svg").size_4()).tooltip("New agent session").dropdown_menu(
        move |menu, _, _| {
          AgentKind::ALL.into_iter().fold(menu, |menu, kind| {
            let workspace = workspace.clone();
            menu.item(
              PopupMenuItem::new(kind.label())
                .icon(Icon::empty().path(kind.icon()))
                .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.new_agent(kind, window, cx))),
            )
          })
        },
      );
    let Some(tabs) = self.agents() else {
      return self.render_agent_empty(&project, cx);
    };
    let strip_tabs = tabs.tabs.iter().enumerate().map(|(index, tab)| {
      let key = tab.session.entity_id().to_string();
      let close = Button::new(SharedString::from(format!("close-agent-tab-{key}")))
        .ghost()
        .xsmall()
        .icon(Icon::empty().path("icons/x.svg").size_4())
        .tooltip("Close")
        .on_click(cx.listener(move |this, _, window, cx| {
          // The click must not also reach the tab and select it.
          cx.stop_propagation();
          this.close_agent_tab(index, window, cx);
        }));
      let label = h_flex().gap_1p5().items_center().child(Icon::empty().path(tab.kind.icon()).size_4()).child(tab.kind.label());
      strip_tab(SharedString::from(format!("agent-tab-{key}")), label, index == tabs.active, close, cx)
        .on_click(cx.listener(move |this, _, window, cx| this.select_agent_tab(index, window, cx)))
    });
    let header = strip(cx)
      .child(h_flex().flex_1().min_w_0().h_full().children(strip_tabs))
      .child(div().px_1().child(picker))
      .child(Label::new(project).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground).pr_3());
    let shown = tabs.current().map(|tab| tab.session.clone());
    v_flex().size_full().child(header).child(div().flex_1().min_h_0().bg(palette.background).children(shown)).into_any_element()
  }

  /// The AI view with no session: the agents to start.
  fn render_agent_empty(&self, project: &SharedString, cx: &mut Context<Self>) -> AnyElement {
    let buttons = AgentKind::ALL.map(|kind| {
      let button = Button::new(SharedString::from(format!("agent-start-{}", kind.command())))
        .outline()
        .icon(Icon::empty().path(kind.icon()).size_4())
        .label(SharedString::from(format!("Start {}", kind.label())))
        .tooltip(kind.description());
      match kind {
        AgentKind::Codex => button.on_click(cx.listener(|this, _, window, cx| this.new_codex(&NewCodex, window, cx))),
        AgentKind::Claude => button.on_click(cx.listener(|this, _, window, cx| this.new_claude(&NewClaude, window, cx))),
      }
    });
    Empty::new()
      .size_full()
      .child(
        EmptyHeader::new()
          .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(Icon::empty().path("icons/bot-message-square.svg")))
          .title(EmptyTitle::new().child("AI agents"))
          .description(EmptyDescription::new().child(SharedString::from(format!("Start Codex or Claude in {project}.")))),
      )
      .child(h_flex().gap_2().children(buttons))
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

/// What is waiting in a project, for its row in the project menu and its tab in the title bar: commits to push (an arrow), and
/// files with changes (a dot). `on_accent` draws them for the open project's tab, which is filled with the accent color.
fn project_status_marks(scope: &str, path: &Path, status: Option<&ProjectStatus>, on_accent: bool, cx: &App) -> Vec<AnyElement> {
  let palette = colors(cx);
  let (push, changed) = status.map_or((false, 0), |status| (status.push, status.changed));
  let dot = if on_accent { palette.on_accent } else { cx.theme().primary };
  let arrow = if on_accent { palette.on_accent } else { cx.theme().muted_foreground };
  let mut marks = Vec::new();
  if push {
    marks.push(
      div()
        .id(SharedString::from(format!("{scope}-push-{}", path.display())))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .when(!on_accent, |mark| mark.px_2().rounded_full().bg(palette.hover))
        .tooltip(|window, cx| Tooltip::new("Commits waiting to be pushed").build(window, cx))
        .child(Icon::empty().path("icons/arrow-up.svg").size_3p5().text_color(arrow))
        .into_any_element(),
    );
  }
  if changed > 0 {
    marks.push(
      div()
        .id(SharedString::from(format!("{scope}-changes-{}", path.display())))
        .flex_none()
        .size_2()
        .rounded_full()
        .bg(dot)
        .tooltip(move |window, cx| {
          Tooltip::new(if changed == 1 { "1 uncommitted change".to_string() } else { format!("{changed} uncommitted changes") }).build(window, cx)
        })
        .into_any_element(),
    );
  }
  marks
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
    .children(project_status_marks("list", &path, status, false, cx))
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
/// A small blue circle with a count (99+ above that), for the files with changes.
fn count_badge(count: usize, cx: &App) -> Div {
  let palette = colors(cx);
  div()
    .min_w_4()
    .h_4()
    .px_1()
    .flex()
    .flex_none()
    .items_center()
    .justify_center()
    .rounded_full()
    .bg(cx.theme().primary)
    .text_color(palette.on_accent)
    .text_size(BADGE_FONT_SIZE)
    .child(if count > 99 { "99+".to_string() } else { count.to_string() })
}

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
    .role(gpui_kit::accesskit::Role::Button)
    .aria_label(tooltip)
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
fn change_kind_icon(kind: vekro_git::ChangeKind) -> &'static str {
  match kind {
    vekro_git::ChangeKind::New => "icons/file-plus.svg",
    vekro_git::ChangeKind::Modified => "icons/file-pen-line.svg",
    vekro_git::ChangeKind::Deleted => "icons/file-minus.svg",
    vekro_git::ChangeKind::Renamed => "icons/file-symlink.svg",
    vekro_git::ChangeKind::Conflicted => "icons/file-exclamation-point.svg",
  }
}

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
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let commit_shown = self.sidebar_tab == SidebarTab::Git && self.git_view == GitView::History && self.selected_commit.is_some();
    let change_shown = self.sidebar_tab == SidebarTab::Git && self.git_view == GitView::Changes && self.selected_change.is_some();
    let editor_area: AnyElement = if commit_shown {
      self.commit_view.clone().into_any_element()
    } else if self.sidebar_tab == SidebarTab::Git && self.git_view == GitView::Changes && self.all_changes_open {
      self.all_changes.clone().into_any_element()
    } else if change_shown {
      self.change_view.clone().into_any_element()
    } else if self.sidebar_tab == SidebarTab::Git && self.git_view == GitView::Changes {
      let changed = self.git.as_ref().map_or(0, Summary::changed);
      let (title, description): (_, SharedString) = if changed == 0 {
        let description = match self.git.as_ref().and_then(|git| git.sync.as_ref()).filter(|sync| sync.ahead > 0) {
          Some(sync) => {
            format!("{} local commit{} ready to push to {}.", sync.ahead, if sync.ahead == 1 { " is" } else { "s are" }, sync.remote).into()
          }
          None => "The working tree has no uncommitted changes.".into(),
        };
        ("No local changes", description)
      } else {
        ("Select a changed file", "Choose a file in Changes to preview its diff.".into())
      };
      Empty::new()
        .size_full()
        .child(
          EmptyHeader::new()
            .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(Icon::empty().path("icons/file-diff.svg")))
            .title(EmptyTitle::new().child(title))
            .description(EmptyDescription::new().child(description)),
        )
        .into_any_element()
    } else {
      self.dock.clone().into_any_element()
    };
    // The terminals take the whole main area, like a terminal window; the sidebar stays beside it.
    let main_area = if self.terminal_open {
      self.render_terminal_area(cx).into_any_element()
    } else if self.agent_open {
      self.render_agent_area(cx)
    } else {
      editor_area
    };
    v_flex()
      .id("workspace")
      .key_context("Workspace")
      .track_focus(&self.focus_handle)
      .when(self.full_area_open(), |this| {
        let command_held = window.modifiers().platform;
        this.on_modifiers_changed(move |event, window, _| {
          if event.modifiers.platform != command_held {
            window.refresh();
          }
        })
      })
      .on_action(cx.listener(Self::add_project))
      .on_action(cx.listener(Self::open_settings))
      .on_action(cx.listener(Self::check_for_updates))
      .on_action(cx.listener(Self::search_project))
      .on_action(cx.listener(Self::open_search_results))
      .on_action(cx.listener(Self::new_terminal))
      .on_action(cx.listener(Self::toggle_terminal))
      .on_action(cx.listener(Self::toggle_agents))
      .on_action(cx.listener(Self::new_codex))
      .on_action(cx.listener(Self::new_claude))
      .on_action(cx.listener(Self::toggle_all_changes))
      .on_action(cx.listener(Self::open_changed_file))
      .on_action(cx.listener(Self::split_right))
      .on_action(cx.listener(Self::view_on_host))
      .on_action(cx.listener(Self::split_down))
      .on_action(cx.listener(Self::save_file))
      .on_action(cx.listener(Self::close_tab))
      .size_full()
      .bg(theme.background)
      .text_color(theme.foreground)
      .text_size(FONT_SIZE)
      .child(
        TitleBar::new().h(TOOLBAR_HEIGHT).pl(px(96.)).bg(palette.rail).border_b_1().border_color(palette.border).child(
          h_flex()
            .size_full()
            .items_center()
            .gap_2()
            .pr_3()
            .child(self.render_repository_switcher(cx))
            .child(div().w_px().h_5().bg(palette.border))
            .children(self.render_favorite_projects(cx))
            .child(rail_button("title-add", Icon::new(IconName::Plus), "Add project", false, false, &palette, |_, window, cx| {
              window.dispatch_action(Box::new(AddProject), cx)
            })),
        ),
      )
      .child(h_flex().flex_1().min_h_0().child(self.render_activity_bar(cx)).child(if self.full_area_open() {
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
                .min_w_0()
                .overflow_hidden()
                .when(self.sidebar_tab == SidebarTab::Git, |column| column.children(self.render_git_toolbar(cx)))
                .child(div().flex_1().min_h_0().min_w_0().w_full().overflow_hidden().child(main_area))
                // The Git view has the branch in its own bar: no status bar there.
                .when(self.sidebar_tab != SidebarTab::Git, |column| column.child(self.render_status_bar(cx))),
            ),
          )
          .into_any_element()
      }))
  }
}

/// How far one step of a push, pull or fetch has got, from the percentage in git's progress line: the steps weigh differently
/// ("Receiving objects: 45% (20/46)" is the middle of a fetch, "Resolving deltas" its end).
fn progress_fraction(line: &str) -> Option<f32> {
  let line = line.rsplit(['\r', '\n']).find(|part| part.contains('%')).unwrap_or(line);
  let percent: f32 = line.split('%').next()?.rsplit(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?;
  let line = line.to_lowercase();
  let (start, span) = if line.contains("resolving") {
    (0.8, 0.2)
  } else if line.contains("writing") {
    (0.4, 0.6)
  } else if line.contains("receiving") {
    (0.2, 0.6)
  } else if line.contains("compressing") {
    (0.2, 0.2)
  } else {
    (0., 0.2)
  };
  Some(start + span * (percent / 100.).clamp(0., 1.))
}
