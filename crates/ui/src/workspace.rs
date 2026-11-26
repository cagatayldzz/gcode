use crate::job_scope::{self, Draft, Drafts};
use crate::line_comments::CommentMode;
use crate::{
  agents::{AgentKind, AgentTabs},
  all_changes::{AllChanges, OpenChangedFile},
  change_view::ChangeView,
  commit_settings::{CommitSettings, CommitSettingsSaved},
  commit_view::CommitView,
  components::{avatar_look, display_path, selected_style, strip, strip_tab},
  connect_account::{AccountConnected, ConnectAccount},
  connections::{credentials_for_repository, describe, AccountsList},
  editor::{ActiveTab, EditorPanel, FileLocation, FileLocationCommand, OpenLocation},
  explorer, extensions,
  mention_picker::{MentionChoice, MentionPicker, ProjectsChosen},
  merge_dialog::{MergeDialog, MergeEvent},
  profiles::ProfileManager,
  project_manager::ProjectManager,
  project_search,
  project_settings::{AppearanceChosen, HostedRepository, ProjectSettings, ProjectSettingsEvent, RenamedRemote},
  project_setup::{ProjectReady, ProjectSetup},
  pull_request::{PullRequestEvent, PullRequestForm, PullRequestTarget},
  settings::{activity_ids, settings_for, SettingsModel},
  settings_window::SettingsWindow,
  terminal::{Terminal, TerminalActivityChanged, TerminalAttentionChanged, TerminalExited},
  terminal_layout::{Arrangement, Direction, Node},
  theme::{apply_theme, colors, FONT_SIZE, FONT_SIZE_SM},
  updates::{AppUpdate, UpdatePanel},
  AddLineComment, AddProfile, AddProject, CheckForUpdates, ClearBuffer, ClosePane, CloseTab, DisableProjectLanguageServers, EditProfiles,
  EnableProjectLanguageServers, FindInFile, MentionProjects, NewClaude, NewCodex, NewTerminal, OpenSettings, RemoveLineComment, ReplaceInFile,
  SearchProject, SelectNextOccurrence, SplitDown, SplitRight, ToggleAgents, ToggleAllChanges, ToggleLineComment, ToggleProjectSettings,
  ToggleTerminal, ViewOnHost,
};
use gpui_kit::{base::Selectable as _, prelude::FluentBuilder as _};
use gpui_kit::{
  component::{
    button::{Button, ButtonCustomVariant, ButtonVariant, ButtonVariants as _},
    checkbox::Checkbox,
    dock::{panel_handle, DockArea, DockLayout, DockSkin},
    empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle},
    h_flex,
    input::{Input, InputEvent, InputState, TextareaState},
    kbd::Kbd,
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
    v_flex, v_virtual_list, ActiveTheme, Disableable as _, Icon, IconName, Sizable as _, TitleBar, VirtualListScrollHandle, WindowExt as _,
  },
  *,
};
use std::{
  cell::{Cell, RefCell},
  collections::{HashMap, HashSet},
  ops::Range,
  path::{Path, PathBuf},
  rc::Rc,
  sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
  },
  time::{Duration, Instant},
};
use ws_commitgen::DEFAULT_INSTRUCTIONS;
use ws_git::{BranchChoices, BranchRef, CommitInfo, Comparison, Summary};
use ws_hosting::{Connection, PullRequest, RepoRef, TokenStore as _};
use ws_storage::{Category, Profile, Project, ProjectList, WindowMode, WindowPlacement};

mod accounts;
mod branches;
mod changes;
mod command_palette;
mod commit;
mod editor_save;
mod entries;
mod file_actions;
mod git;
mod helpers;
mod missing_project;
mod profiles;
mod projects;
mod quick_open;
mod render_git;
mod render_menus;
mod render_panels;
mod render_problems;
mod search;
mod terminals;

use file_actions::FileActions;
use helpers::*;

/// Height of the top band: the traffic lights and the title.
pub const TOOLBAR_HEIGHT: Pixels = px(40.);

/// The count on the Git button: how small its digits are, and how far it reaches past the button's right edge.
const BADGE_FONT_SIZE: Pixels = px(10.);
const BADGE_OVERHANG: Pixels = px(4.);

/// The width of the bar at the far left that chooses the view of the sidebar.
const ACTIVITY_BAR_WIDTH: Pixels = px(56.);

/// The width of the branch menu.
const MENU_WIDTH: Pixels = px(380.);

/// The width of the project menu: wider, as each project also shows what is waiting in it.
const PROJECT_MENU_WIDTH: Pixels = px(380.);

/// How tall the lists of the project menu and of the branch menu get before they scroll.
const MENU_LIST_MAX_HEIGHT: Pixels = px(320.);

/// How tall the branch list gets before it scrolls. It is taller than the project menu's lists because a repository has many
/// branches; a short window still caps it.
const BRANCH_LIST_MAX_HEIGHT: Pixels = px(560.);

/// The line under the Git bar: the branch menu starts below it, so that it does not cover it.
const BAR_BORDER: Pixels = px(1.);

/// The space between the Git bar and the branch menu that floats under it.
const MENU_GAP: Pixels = px(4.);

/// How many of the most recently changed branches the branch menu lists as recent.
const RECENT_BRANCHES: usize = 5;

/// The branches of the compare list, in its groups.
#[derive(Default)]
pub(crate) struct CompareGroups {
  pub default: Option<BranchRef>,
  pub favorites: Vec<BranchRef>,
  pub recent: Vec<BranchRef>,
  pub other: Vec<BranchRef>,
}

/// How often the toolbar redraws while git reports its progress.
/// How long the fill of a finished push, pull or fetch takes to reach the right edge.
const REMOTE_FINISH: Duration = Duration::from_millis(300);

/// How often the terminal's activity dot fades once while a command runs, and how long that fade lasts. Rare on purpose: it only shows that
/// the work goes on, and nothing redraws between the fades.
const TERMINAL_PULSE_INTERVAL: Duration = Duration::from_secs(10);
const TERMINAL_PULSE: Duration = Duration::from_millis(1200);

/// One blink of the terminal icon on a project's tab.
const TERMINAL_BLINK: Duration = Duration::from_millis(1400);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

/// How long the pointer must rest on a file in the Explorer before it is prepared for opening, so that moving past
/// files costs nothing.
const HOVER_PREPARE_DELAY: Duration = Duration::from_millis(120);

/// How often the Git summary and the folder tree are read again while the window is open.
const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// While the window is not the one in front, only every this many rounds of [`REFRESH_INTERVAL`] read the repository (coming back
/// to the window reads it at once).
const BACKGROUND_REFRESH_EVERY: u32 = 5;

/// A routine refresh that takes at least this long (milliseconds) is written to the activity log; faster ones are not.
pub(crate) const REFRESH_NOTABLE_MS: u128 = 150;

/// How often the changes of the favorite projects are read for their tabs in the title bar.
const FAVORITE_STATUS_INTERVAL: Duration = Duration::from_secs(10);

/// Width of the sidebar when the window opens.
/// The activity log panel at the bottom: its height when opened and the range the user can drag it through.
const ACTIVITY_LOG_HEIGHT: Pixels = px(280.);
const ACTIVITY_LOG_MIN_HEIGHT: Pixels = px(120.);
const ACTIVITY_LOG_MAX_HEIGHT: Pixels = px(720.);

const SIDEBAR_WIDTH: Pixels = px(280.);

/// How narrow and how wide the sidebar can be dragged.
const SIDEBAR_MIN_WIDTH: Pixels = px(200.);
const SIDEBAR_MAX_WIDTH: Pixels = px(960.);

/// Indentation of each tree level: a child's arrow sits a little right of its parent's, not a full arrow's width.
const INDENT: Rems = rems(0.75);

/// The views of the sidebar, in activity-bar order.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum SidebarTab {
  #[default]
  Explorer,
  Search,
  Git,
  Extensions,
}

impl SidebarTab {
  /// The id the activity bar layout in the preferences knows the button by.
  fn id(self) -> &'static str {
    match self {
      SidebarTab::Explorer => "explorer",
      SidebarTab::Search => "search",
      SidebarTab::Git => "git",
      SidebarTab::Extensions => "extensions",
    }
  }

  const ALL: [SidebarTab; 4] = [SidebarTab::Explorer, SidebarTab::Search, SidebarTab::Git, SidebarTab::Extensions];

  fn icon(self) -> Icon {
    match self {
      SidebarTab::Explorer => Icon::empty().path("icons/folder-tree.svg"),
      SidebarTab::Search => Icon::new(IconName::Search),
      SidebarTab::Git => Icon::empty().path("icons/git-branch.svg"),
      SidebarTab::Extensions => Icon::empty().path("icons/blocks.svg"),
    }
  }

  fn label(self) -> &'static str {
    match self {
      SidebarTab::Explorer => "Explorer",
      SidebarTab::Search => "Search",
      SidebarTab::Git => "Git",
      SidebarTab::Extensions => "Extensions",
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

struct SearchCancellation(Arc<AtomicBool>);

impl Drop for SearchCancellation {
  fn drop(&mut self) {
    self.0.store(true, Ordering::Relaxed);
  }
}

/// What the window shows for a project that is not open at the moment.
#[derive(Default)]
struct ProjectView {
  sidebar_tab: SidebarTab,
  git_view: GitView,
  terminal_open: bool,
  agent_open: bool,
  expanded: HashSet<PathBuf>,
  selected_entry: Option<PathBuf>,
  selected_change: Option<PathBuf>,
}

/// A push, pull or fetch that is running: what the toolbar button says, and the last progress line git reported.
struct RemoteWork {
  /// The project the work belongs to: the window may show another one by the time it ends.
  root: PathBuf,
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
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum GitView {
  #[default]
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
  /// The profile this window shows: its projects, settings and accounts.
  profile: Profile,
  projects: ProjectList,
  /// The settings of the profile, shared by its windows.
  settings: Entity<SettingsModel>,
  git: Option<Summary>,
  expanded: HashSet<PathBuf>,
  rows: Vec<explorer::Row>,
  selected_entry: Option<PathBuf>,
  /// Entries picked together with Shift or Command; empty while a single entry is selected.
  explorer_selection: HashSet<PathBuf>,
  /// Where a Shift range starts.
  selection_anchor: Option<PathBuf>,
  /// The changed file whose diff is open, relative to the repository.
  selected_change: Option<PathBuf>,
  /// The activity and file selections of the projects that are not open.
  views: HashMap<PathBuf, ProjectView>,
  /// The commit messages typed in the projects that are not open.
  drafts: Drafts,
  /// The view the sidebar shows.
  sidebar_tab: SidebarTab,
  /// The file in the Explorer the pointer is on.
  hovered_file: Option<PathBuf>,
  hovered_folder: Option<(PathBuf, usize)>,
  focus_handle: FocusHandle,
  _setup_subscription: Option<Subscription>,
  _dialog_subscription: Option<Subscription>,
  /// The project settings view, which takes the main area like the terminals do while it is open.
  project_settings: Option<Entity<ProjectSettings>>,
  _project_settings_subscription: Option<Subscription>,
  /// The application's Settings, which take the main area the same way while they are open.
  app_settings: Option<Entity<SettingsWindow>>,
  /// Projects whose repository was renamed on its service, found by asking it, with what the remote should say now.
  renamed_remotes: HashMap<PathBuf, RenamedRemote>,
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
  /// The changed files chosen together in the Changes list (⌘-click, ⇧-click), and where a ⇧-click range starts.
  chosen_changes: HashSet<PathBuf>,
  choice_anchor: Option<PathBuf>,
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
  /// What `ws_git::history_stamp` said at the last refresh: the History list is read again when it changes.
  history_stamp: Option<String>,
  /// The project, page size and stamp the commits in `history` were read at: switching to History again reads nothing while they match.
  history_loaded: Option<(PathBuf, usize, String)>,
  /// Whether the commits are being read, so that the History list shows a loader.
  history_loading: bool,
  /// The branch being switched to, while the switch runs: the branch button, the changes and the commits show a loader until it is done.
  switching_branch: Option<String>,
  /// How many commits the History view asks for now, and its scroll position (to load more near the end).
  history_limit: usize,
  history_scroll: ScrollHandle,
  /// The keyboard goes to the Changes or the History list when it is clicked, so that ↑ and ↓ choose a row.
  changes_focus: FocusHandle,
  history_focus: FocusHandle,
  /// The scroll position of the list in the branch menu, which has a scrollbar strip of its own.
  branch_scroll: ScrollHandle,
  /// The scroll position of the branches under the compare field, which have a scrollbar strip of their own.
  compare_scroll: ScrollHandle,
  changes_scroll: VirtualListScrollHandle,
  /// Row sizes of the virtualized Changes list.
  change_sizes: RefCell<Rc<Vec<Size<Pixels>>>>,
  /// Makes the open project's index ready in the background; a newer project replaces (cancels) it.
  index_task: Task<()>,
  /// The window is the one in front, and what the Repository menu said last.
  window_active: bool,
  menu_host: Option<String>,
  /// The profiles the Profiles menu lists now.
  menu_profiles: Vec<Profile>,
  /// The terminals fill the main area (the right part of the window) in place of the editor.
  /// The Problems panel is open under the editor.
  problems_open: bool,
  /// The activity log panel at the bottom of the window (the debug button), while it is open, with what hears its close button.
  activity_log: Option<(Entity<crate::activity_panel::ActivityLog>, Subscription)>,
  terminal_open: bool,
  /// The AI view (Codex, Claude) fills the main area, like the terminals do; the two never show together.
  agent_open: bool,
  /// The project tab that was last picked up; it counts as held while the pointer still drags something.
  dragged_tab: Option<PathBuf>,
  /// Where each project tab was last drawn, to slide the tabs along the bar when two of them swap.
  tab_bounds: Rc<RefCell<HashMap<PathBuf, Bounds<Pixels>>>>,
  /// How far each tab has to travel back to where it was before a swap (in pixels), and the swap's number, which restarts its animation.
  tab_slides: HashMap<PathBuf, (f32, u64)>,
  tab_swaps: u64,
  /// When the tabs last swapped: they do not swap again while they glide, which would send them back and forth.
  tab_swapped_at: Option<Instant>,
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
  remote_finish: Option<(f32, usize, PathBuf)>,
  remote_runs: usize,
  /// The branch menu is open, with the local branches it lists and the field that filters them.
  branch_menu_open: bool,
  /// Where the branch button is in the window, so that its menu can reach from the button's bottom to the window's.
  branch_trigger_bounds: Rc<Cell<Bounds<Pixels>>>,
  branches: Vec<ws_git::BranchEntry>,
  branch_filter: Entity<InputState>,
  /// Narrows the list of changed files by name.
  changes_filter: Entity<InputState>,
  /// The Extensions view of the sidebar.
  extensions_panel: Entity<extensions::ExtensionsPanel>,
  /// The page of the extension chosen in the Extensions view.
  extension_page: Entity<extensions::ExtensionPage>,
  /// The repository menu of the title bar is open.
  projects_menu_open: bool,
  project_filter: Entity<InputState>,
  projects_menu_scroll: ScrollHandle,
  /// Writes where the window is once it has stopped moving or resizing; a newer change replaces (cancels) the pending write.
  window_save: Task<()>,
  /// The latest placement, written at once when the window closes or the app quits, so that a move right before a restart is not lost.
  placement: Rc<Cell<Option<WindowPlacement>>>,
  _window_bounds: Subscription,
  _placement_on_quit: Subscription,
  _placement_on_close: Subscription,
  _commit_subscriptions: Vec<Subscription>,
  _settings_subscriptions: Vec<Subscription>,
  _appearance: Subscription,
  _activation: Subscription,
  /// Repaints the window when the editor changes: the status bar shows the cursor.
  _editor: Subscription,
  /// Selects in the sidebar what the editor area shows.
  _active_tab: Subscription,
  /// Opens the file a ⌘-click on a name led to.
  _open_location: Subscription,
  _refresh_task: Task<()>,
  /// Counts the fades of the terminal's activity dot; a new count restarts its animation.
  terminal_pulse: usize,
  _terminal_pulse_task: Task<()>,
  /// A check for projects whose folder was deleted is running.
  syncing: bool,
  /// The listed projects whose folder is gone, found by the last check; they stay listed, and opening one says so.
  missing_projects: HashSet<PathBuf>,
  /// A missing project is being cloned again.
  recovering: bool,
  dock: Entity<DockArea>,
  editor_panel: Entity<EditorPanel>,
  editor_saves: HashMap<PathBuf, Task<()>>,
  save_dialog_task: Option<Task<()>>,
  file_actions: FileActions,
  /// The shells of every project that has been opened, in tabs and split panes, kept alive while another project is shown.
  terminal_tabs: HashMap<PathBuf, Arrangement<Entity<Terminal>>>,
  _terminal_subscriptions: Vec<Subscription>,
}

impl Workspace {
  pub fn new(profile: Profile, window: &mut Window, cx: &mut Context<Self>) -> Self {
    // The first window of a profile loads its settings; later windows of it share them.
    let settings = settings_for(&profile, cx);
    window.set_window_title(&format!("{} — {}", ws_product::NAME, profile.name));
    {
      let preferences = &settings.read(cx).preferences;
      apply_theme(preferences.theme, preferences.font, preferences.code_font_size, window, cx);
    }
    let placement = Rc::new(Cell::new(None));
    let (dock, _skin) = DockSkin::dock_area("workspace-dock", Some(1), window, cx);
    let editor_panel = cx.new(|cx| EditorPanel::new(window, cx));
    let projects = ProjectList::load(&profile.projects_path());
    editor_panel.update(cx, |editor, cx| editor.start_recovery(&profile, &projects, window, cx));
    dock.update(cx, |area, cx| {
      area.set_center(DockLayout::h_split().child(DockLayout::tabs().panel_view(panel_handle(editor_panel.clone()), cx), None), window, cx);
    });
    let commit_view = cx.new({
      let settings = settings.clone();
      |_| CommitView::new(settings)
    });
    let change_view = cx.new({
      let settings = settings.clone();
      |_| ChangeView::new(settings)
    });
    let commit_summary = cx.new(|cx| InputState::new(window, cx).placeholder("Summary (required)"));
    let commit_description = cx.new(|cx| TextareaState::new(window, cx).placeholder("Description"));
    let branch_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find or create a branch"));
    let changes_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter"));
    let extensions_panel = cx.new(|cx| extensions::ExtensionsPanel::new(window, cx));
    let extension_page = cx.new(|cx| extensions::ExtensionPage::new(extensions_panel.clone(), cx));
    let project_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a project"));
    let compare_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Select Branch to Compare…"));
    let mut this = Self {
      projects,
      profile,
      settings: settings.clone(),
      git: None,
      expanded: HashSet::new(),
      rows: Vec::new(),
      selected_entry: None,
      explorer_selection: HashSet::new(),
      selection_anchor: None,
      selected_change: None,
      sidebar_tab: SidebarTab::default(),
      hovered_file: None,
      hovered_folder: None,
      views: HashMap::new(),
      drafts: Drafts::default(),
      focus_handle: cx.focus_handle(),
      _setup_subscription: None,
      _dialog_subscription: None,
      project_settings: None,
      _project_settings_subscription: None,
      app_settings: None,
      renamed_remotes: HashMap::new(),
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
      chosen_changes: HashSet::new(),
      choice_anchor: None,
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
      commit_view: commit_view.clone(),
      change_view: change_view.clone(),
      all_changes: cx.new({
        let settings = settings.clone();
        |_| AllChanges::new(settings)
      }),
      all_changes_open: false,
      pull_requests: PullRequests::Idle,
      pr_epoch: 0,
      history_epoch: 0,
      history_stamp: None,
      history_loaded: None,
      history_loading: false,
      switching_branch: None,
      history_limit: HISTORY_PAGE,
      history_scroll: ScrollHandle::new(),
      changes_focus: cx.focus_handle(),
      history_focus: cx.focus_handle(),
      branch_scroll: ScrollHandle::new(),
      compare_scroll: ScrollHandle::new(),
      changes_scroll: VirtualListScrollHandle::new(),
      change_sizes: RefCell::new(Rc::new(Vec::new())),
      index_task: Task::ready(()),
      window_active: true,
      menu_host: None,
      menu_profiles: Vec::new(),
      problems_open: false,
      activity_log: None,
      terminal_open: false,
      agent_open: false,
      dragged_tab: None,
      tab_bounds: Rc::default(),
      tab_slides: HashMap::new(),
      tab_swaps: 0,
      tab_swapped_at: None,
      agent_tabs: HashMap::new(),
      generating: false,
      project_status: HashMap::new(),
      project_status_read: None,
      remote_work: None,
      remote_finish: None,
      remote_runs: 0,
      branch_menu_open: false,
      branch_trigger_bounds: Rc::default(),
      branches: Vec::new(),
      branch_filter: branch_filter.clone(),
      changes_filter: changes_filter.clone(),
      extensions_panel: extensions_panel.clone(),
      extension_page,
      projects_menu_open: false,
      project_filter: project_filter.clone(),
      projects_menu_scroll: ScrollHandle::new(),
      window_save: Task::ready(()),
      placement: placement.clone(),
      _window_bounds: cx.observe_window_bounds(window, |this, window, cx| this.remember_window(window, cx)),
      _placement_on_quit: cx.on_app_quit(|this, cx| {
        this.save_placement_now();
        if let Err(error) = this.editor_panel.update(cx, |editor, cx| editor.flush_recovery(cx)) {
          eprintln!("Could not checkpoint unsaved edits: {error}");
        }
        async {}
      }),
      _placement_on_close: {
        let window_id = window.window_handle().window_id();
        cx.on_window_closed(move |_, closed| {
          if closed == window_id {
            projects::save_placement(&placement);
          }
        })
      },
      _commit_subscriptions: vec![
        // A double click on a file of a commit opens the file in the editor with the Explorer.
        cx.subscribe_in(&commit_view, window, |this, _, event: &crate::commit_view::OpenFileRequested, window, cx| {
          this.open_change_in_editor(&event.0, window, cx);
        }),
        // A double click on a line of a diff opens the file at that line.
        cx.subscribe_in(&change_view, window, |this, _, event: &crate::change_view::OpenInEditor, window, cx| {
          this.open_change_line_in_editor(&event.file, event.line, window, cx);
        }),
        // The commit button is enabled once there is a summary.
        cx.subscribe(&commit_summary, |_, _, _: &InputEvent, cx| cx.notify()),
        // The menus list what the filter field matches.
        cx.subscribe(&branch_filter, |_, _, _: &InputEvent, cx| cx.notify()),
        cx.subscribe(&changes_filter, |_, _, _: &InputEvent, cx| cx.notify()),
        // Removing the AI Agent extension closes the AI view if it was open.
        cx.subscribe(&extensions_panel, |this, _, _: &extensions::ExtensionsChanged, cx| {
          if this.agent_open && extensions::enabled_agents(cx).is_empty() {
            this.agent_open = false;
          }
          cx.notify();
        }),
        // Choosing an extension shows its page in the main area.
        cx.subscribe(&extensions_panel, |_, _, _: &extensions::ExtensionSelected, cx| cx.notify()),
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
          this.continue_update(window, cx);
          let preferences = settings.read(cx).preferences.clone();
          if let Err(error) = preferences.save(&settings.read(cx).profile.preferences_path()) {
            eprintln!("Could not save the settings: {error}");
          }
          apply_theme(preferences.theme, preferences.font, preferences.code_font_size, window, cx);
          cx.notify();
        }),
      ],
      dock,
      editor_panel: editor_panel.clone(),
      editor_saves: HashMap::new(),
      save_dialog_task: None,
      file_actions: FileActions::default(),
      terminal_tabs: HashMap::new(),
      _terminal_subscriptions: Vec::new(),
      _appearance: cx.observe_window_appearance(window, |this, window, cx| {
        let preferences = &this.settings.read(cx).preferences;
        apply_theme(preferences.theme, preferences.font, preferences.code_font_size, window, cx)
      }),
      // Changes made in another app (an editor, another Git client, a terminal) show up as soon as the window is back.
      _editor: cx.observe(&editor_panel, |_, _, cx| cx.notify()),
      _active_tab: cx.subscribe_in(&editor_panel, window, |this, editor, shown: &ActiveTab, window, cx| {
        match shown {
          ActiveTab::File(path) => {
            editor.update(cx, |editor, cx| editor.sync_shown_file(path, window, cx));
            this.selected_entry = Some(path.clone());
            this.selected_change = None;
            // The Explorer shows where the file is: its folders open and the tree scrolls to it only if it is out of sight.
            if let Some(root) = this.projects.active.clone() {
              this.show_explorer_entry(&root, path.clone(), ScrollStrategy::Nearest, cx);
            }
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
      _open_location: cx.subscribe_in(&editor_panel, window, |this, _, target: &OpenLocation, window, cx| {
        let path = target.path.clone();
        this.editor_panel.update(cx, |editor, _| editor.allow_large(&path));
        this.open_file(path.clone(), window, cx);
        this.editor_panel.update(cx, |editor, cx| editor.reveal_range(&path, target.start, target.end, window, cx));
      }),
      _activation: cx.observe_window_activation(window, |this, window, cx| {
        this.window_active = window.is_window_active();
        if window.is_window_active() {
          let id = this.profile.id.clone();
          crate::profiles::update_profiles(cx, |list| list.set_last(&id));
          this.sync_menus(true, cx);
          this.announce_update(window, cx);
          this.refresh_in_background(cx);
          this.sync_projects(window, cx);
        }
      }),
      _refresh_task: cx.spawn_in(window, async move |this, cx| {
        let mut round = 0u32;
        loop {
          cx.background_executor().timer(REFRESH_INTERVAL).await;
          round = round.wrapping_add(1);
          let refreshed = this.update_in(cx, |this, window, cx| {
            if this.window_active || round.is_multiple_of(BACKGROUND_REFRESH_EVERY) {
              this.refresh_in_background(cx);
              this.sync_projects(window, cx);
            }
          });
          if refreshed.is_err() {
            break;
          }
        }
      }),
      terminal_pulse: 0,
      _terminal_pulse_task: cx.spawn_in(window, async move |this, cx| loop {
        cx.background_executor().timer(TERMINAL_PULSE_INTERVAL).await;
        let pulsed = this.update_in(cx, |this, _, cx| {
          if this.projects.active.as_ref().is_some_and(|root| this.project_has_terminal_activity(root, cx)) {
            this.terminal_pulse += 1;
            cx.notify();
          }
        });
        if pulsed.is_err() {
          break;
        }
      }),
      syncing: false,
      missing_projects: HashSet::new(),
      recovering: false,
    };
    this.sync_projects(window, cx);
    this.check_remote_rename(cx);
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

  /// Close Pane (menus only): closes the terminal pane that has the keyboard.
  pub fn close_focused_pane(&mut self, _: &ClosePane, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(terminal) = self.focused_terminal(window, cx) {
      self.close_pane(terminal, window, cx);
    }
  }

  /// ⌘W: closes the open tab of the editor, then the others from the last to the first, and keeps the focus in the window so
  /// the shortcut keeps working. It never closes the window, and does nothing while a terminal has the keyboard: a running
  /// terminal is only closed on purpose, from its menu or its close button.
  pub fn close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
    if self.terminal_focused(window, cx) {
      return;
    }
    if let Some(side) = self.file_actions.focused_side(window, cx) {
      side.update(cx, |editor, cx| editor.close_by_shortcut(cx));
      self.focus_handle.focus(window, cx);
      return;
    }
    if self.editor_panel.update(cx, |editor, cx| editor.close_by_shortcut(cx)) {
      // The closed tab held the focus; without a focused element in the window the next ⌘W would reach nothing.
      self.focus_handle.focus(window, cx);
    }
  }

  /// Add Line Comment (⌘K ⌘C): comments out the selected lines of the file in the editor.
  pub fn add_line_comment(&mut self, _: &AddLineComment, window: &mut Window, cx: &mut Context<Self>) {
    self.comment_lines(CommentMode::Add, window, cx);
  }

  /// Remove Line Comment (⌘K ⌘U): comments in the selected lines.
  pub fn remove_line_comment(&mut self, _: &RemoveLineComment, window: &mut Window, cx: &mut Context<Self>) {
    self.comment_lines(CommentMode::Remove, window, cx);
  }

  /// Toggle Line Comment (⌘/): comments the selected lines out, or in when they all are comments already.
  pub fn toggle_line_comment(&mut self, _: &ToggleLineComment, window: &mut Window, cx: &mut Context<Self>) {
    self.comment_lines(CommentMode::Toggle, window, cx);
  }

  /// Add Next Occurrence (⌘D in the editor): see `EditorPanel::select_next_occurrence`.
  pub fn select_next_occurrence(&mut self, _: &SelectNextOccurrence, window: &mut Window, cx: &mut Context<Self>) {
    self.editor_panel.update(cx, |editor, cx| editor.select_next_occurrence(window, cx));
  }

  /// Find (⌘F in the editor): see `EditorPanel::open_find`.
  pub fn find_in_file(&mut self, _: &FindInFile, window: &mut Window, cx: &mut Context<Self>) {
    self.editor_panel.update(cx, |editor, cx| editor.open_find(false, window, cx));
  }

  /// Replace (⌥⌘F in the editor): the find widget with the replace row open.
  pub fn replace_in_file(&mut self, _: &ReplaceInFile, window: &mut Window, cx: &mut Context<Self>) {
    self.editor_panel.update(cx, |editor, cx| editor.open_find(true, window, cx));
  }

  fn comment_lines(&mut self, mode: CommentMode, window: &mut Window, cx: &mut Context<Self>) {
    if let Err(message) = self.editor_panel.update(cx, |editor, cx| editor.comment_lines(mode, window, cx)) {
      window.push_notification(Notification::info(message), cx);
    }
  }

  pub(crate) fn flush_editor_recovery(&self, cx: &mut Context<Self>) -> std::io::Result<()> {
    if !self.editor_saves.is_empty() {
      return Err(std::io::Error::other("Wait for the current save to finish"));
    }
    self.editor_panel.update(cx, |editor, cx| editor.flush_recovery(cx))
  }
}

impl Render for Workspace {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let commit_shown = self.sidebar_tab == SidebarTab::Git && self.git_view == GitView::History && self.selected_commit.is_some();
    let change_shown = self.sidebar_tab == SidebarTab::Git && self.git_view == GitView::Changes && self.selected_change.is_some();
    let switching = self.switching_branch.is_some()
      && self.sidebar_tab == SidebarTab::Git
      && (commit_shown || change_shown || (self.git_view == GitView::Changes && self.all_changes_open));
    let editor_area: AnyElement = if switching {
      // The commit or change on screen belongs to the branch being left.
      crate::components::skeleton_page()
    } else if commit_shown {
      self.commit_view.clone().into_any_element()
    } else if self.sidebar_tab == SidebarTab::Extensions && self.extensions_panel.read(cx).has_selection() {
      self.extension_page.clone().into_any_element()
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
    } else if let Some(side) = self.file_actions.visible_side(cx) {
      h_resizable("editor-side-split").child(resizable_panel().child(self.dock.clone())).child(resizable_panel().child(side)).into_any_element()
    } else {
      self.dock.clone().into_any_element()
    };
    // The terminals take the whole main area, like a terminal window; the sidebar stays beside it.
    let main_area = if self.active_project_missing() && !self.editor_panel.read(cx).has_unsaved_tabs() {
      self.render_missing_project(cx)
    } else if let Some(settings) = &self.project_settings {
      settings.clone().into_any_element()
    } else if let Some(settings) = &self.app_settings {
      settings.clone().into_any_element()
    } else if self.terminal_open {
      self.render_terminal_area(cx).into_any_element()
    } else if self.agent_open && !extensions::enabled_agents(cx).is_empty() {
      self.render_agent_area(window, cx)
    } else {
      editor_area
    };
    let terminal_shown = self.project_settings.is_none() && self.app_settings.is_none() && self.terminal_open;
    let content = h_flex()
      .flex_1()
      .min_h_0()
      .child(self.render_activity_bar(cx))
      .child(if self.full_area_open() {
        // The terminals take everything right of the activity bar: no sidebar and no status bar, like a terminal window.
        // The status bar stays under the terminals; the other full-width views have none.
        v_flex()
          .flex_1()
          .min_w_0()
          .h_full()
          .child(div().flex_1().min_h_0().w_full().child(main_area))
          .when(terminal_shown, |column| column.child(self.render_status_bar(cx)))
          .into_any_element()
      } else {
        // The group's content (the all-changes toolbar, for one) has a minimum width; `min_w_0` keeps it from pushing the group past
        // the window edge when the window is made narrow.
        div()
          .flex_1()
          .min_w_0()
          .h_full()
          .overflow_hidden()
          .child(
            h_resizable("sidebar-split")
              // `fixed`: the sidebar keeps the width the user gave it when the window is resized; only the main area changes.
              .child(
                resizable_panel().size(SIDEBAR_WIDTH).size_range(SIDEBAR_MIN_WIDTH..SIDEBAR_MAX_WIDTH).fixed().child(self.render_sidebar(window, cx)),
              )
              // The status bar belongs to the editor: the sidebar runs down to the bottom of the window.
              .child(
                resizable_panel().child(
                  v_flex()
                    .size_full()
                    .min_w_0()
                    .overflow_hidden()
                    .when(self.sidebar_tab == SidebarTab::Git, |column| column.children(self.render_git_toolbar(cx)))
                    .child(div().flex_1().min_h_0().min_w_0().w_full().overflow_hidden().child(main_area))
                    .when(self.problems_open && self.sidebar_tab != SidebarTab::Git, |column| column.children(self.render_problems_panel(cx)))
                    // The Git view has the branch in its own bar: no status bar there.
                    .when(self.sidebar_tab != SidebarTab::Git, |column| column.child(self.render_status_bar(cx))),
                ),
              ),
          )
          .into_any_element()
      })
      .into_any_element();
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
      .on_action(cx.listener(Self::add_profile))
      .on_action(cx.listener(Self::edit_profiles))
      .on_action(cx.listener(Self::check_for_updates))
      .on_action(cx.listener(Self::search_project))
      .on_action(cx.listener(Self::go_to_file))
      .on_action(cx.listener(Self::open_command_palette))
      .on_action(cx.listener(Self::confirm_enable_language_servers))
      .on_action(cx.listener(Self::disable_language_servers))
      .on_action(cx.listener(Self::open_search_results))
      .on_action(cx.listener(Self::new_terminal))
      .on_action(cx.listener(Self::toggle_terminal))
      .on_action(cx.listener(Self::toggle_agents))
      .on_action(cx.listener(Self::toggle_project_settings))
      .on_action(cx.listener(Self::new_codex))
      .on_action(cx.listener(Self::new_claude))
      .on_action(cx.listener(Self::toggle_all_changes))
      .on_action(cx.listener(Self::open_changed_file))
      .on_action(cx.listener(Self::run_file_command))
      .on_action(cx.listener(Self::run_file_location))
      .on_action(cx.listener(Self::split_right))
      .on_action(cx.listener(Self::view_on_host))
      .on_action(cx.listener(Self::split_down))
      .on_action(cx.listener(Self::save_file))
      .on_action(cx.listener(Self::find_in_file))
      .on_action(cx.listener(Self::replace_in_file))
      .on_action(cx.listener(Self::select_next_occurrence))
      .on_action(cx.listener(Self::add_line_comment))
      .on_action(cx.listener(Self::remove_line_comment))
      .on_action(cx.listener(Self::toggle_line_comment))
      .on_action(cx.listener(Self::close_tab))
      .on_action(cx.listener(Self::close_focused_pane))
      .on_action(cx.listener(Self::mention_projects))
      .on_action(cx.listener(Self::show_shortcuts))
      .on_action(cx.listener(Self::select_previous_row))
      .on_action(cx.listener(Self::select_next_row))
      .size_full()
      .bg(theme.background)
      .text_color(theme.foreground)
      .text_size(FONT_SIZE)
      .children(self.render_branch_menu(window, cx))
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
            }))
            .child(div().flex_1())
            .children(self.render_activity_button(cx))
            .children(self.render_update_button(cx)),
        ),
      )
      .child(match &self.activity_log {
        // The activity log panel under everything else; its edge can be dragged.
        Some((log, _)) => div()
          .flex_1()
          .min_h_0()
          .child(v_resizable("activity-split").child(resizable_panel().child(v_flex().size_full().child(content))).child(
            resizable_panel().size(ACTIVITY_LOG_HEIGHT).size_range(ACTIVITY_LOG_MIN_HEIGHT..ACTIVITY_LOG_MAX_HEIGHT).fixed().child(log.clone()),
          ))
          .into_any_element(),
        None => content,
      })
  }
}

/// A tab of the project bar being dragged.
#[derive(Clone)]
struct DraggedProjectTab(PathBuf);

/// The drag of a project tab shows nothing of its own.
struct NoDragPreview;

impl Render for NoDragPreview {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    div()
  }
}
