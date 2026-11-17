use crate::{
  change_view::ChangeView,
  commit_settings::{CommitSettings, CommitSettingsSaved},
  commit_view::CommitView,
  components::{avatar_look, display_path},
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
  terminal::{Terminal, TerminalExited},
  theme::{apply_theme, colors, FONT_SIZE, FONT_SIZE_SM},
  updates::{AppUpdate, UpdatePanel},
  AddProject, CloseTab, NewTerminal, OpenSettings, SaveFile, ToggleTerminal,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonCustomVariant, ButtonVariants as _},
    checkbox::Checkbox,
    dock::{panel_handle, DockArea, DockLayout, DockSkin},
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    label::Label,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem},
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
use vekro_git::{CommitInfo, Summary};
use vekro_hosting::{Connection, KeychainStore, PullRequest, RepoRef, TokenStore as _};
use vekro_opencommit::DEFAULT_INSTRUCTIONS;
use vekro_storage::{Category, Preferences, Project, ProjectList};

/// Height of the top band: the traffic lights and the title.
pub const TOOLBAR_HEIGHT: Pixels = px(40.);

/// The terminal panel's height when it opens, and how far it can be dragged.
const TERMINAL_HEIGHT: Pixels = px(260.);
const TERMINAL_MIN_HEIGHT: Pixels = px(120.);
const TERMINAL_MAX_HEIGHT: Pixels = px(800.);

/// The width of the branch menu and of a category's project list.
const MENU_WIDTH: Pixels = px(280.);

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

/// The two views of the Git tab: the files to commit, and the commits already made.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GitView {
  Changes,
  History,
}

/// How many commits the History view lists.
const HISTORY_LIMIT: usize = 300;

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
  selected_commit: Option<String>,
  commit_view: Entity<CommitView>,
  /// The changes of the chosen file of the Changes view, shown in the main area.
  change_view: Entity<ChangeView>,
  pull_requests: PullRequests,
  /// Which load the listed pull requests belong to, so a slow answer for another project is ignored.
  pr_epoch: u64,
  /// Which read of the history the listed commits belong to.
  history_epoch: u64,
  /// The terminal panel at the bottom of the window is shown.
  terminal_open: bool,
  /// A commit message is being generated.
  generating: bool,
  /// The push, pull or fetch that is running.
  remote_work: Option<RemoteWork>,
  /// The branch menu is open, with the local branches it lists and the field that filters them.
  branch_menu_open: bool,
  branches: Vec<String>,
  branch_filter: Entity<InputState>,
  /// The repository menu of the title bar is open.
  projects_menu_open: bool,
  project_filter: Entity<InputState>,
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
  /// The shells of every project that has been opened, kept alive while another project is shown.
  terminals: HashMap<PathBuf, Vec<Entity<Terminal>>>,
  /// Which of a project's shells the terminal panel shows.
  active_terminal: HashMap<PathBuf, usize>,
  /// The projects whose terminal panel shows two shells side by side.
  split_terminals: HashSet<PathBuf>,
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
      selected_commit: None,
      commit_view: cx.new(|_| CommitView::new()),
      change_view: cx.new(|_| ChangeView::new()),
      pull_requests: PullRequests::Idle,
      pr_epoch: 0,
      history_epoch: 0,
      terminal_open: false,
      generating: false,
      remote_work: None,
      branch_menu_open: false,
      branches: Vec::new(),
      branch_filter: branch_filter.clone(),
      projects_menu_open: false,
      project_filter: project_filter.clone(),
      _commit_subscriptions: vec![
        // The commit button is enabled once there is a summary.
        cx.subscribe(&commit_summary, |_, _, _: &InputEvent, cx| cx.notify()),
        // The menus list what the filter field matches.
        cx.subscribe(&branch_filter, |_, _, _: &InputEvent, cx| cx.notify()),
        cx.subscribe(&project_filter, |_, _, _: &InputEvent, cx| cx.notify()),
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
      terminals: HashMap::new(),
      active_terminal: HashMap::new(),
      split_terminals: HashSet::new(),
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
        if window.is_window_active() {
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

  /// Starts a shell in the open project if it has none: a project never stays without one.
  fn ensure_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    if self.terminals.get(&root).is_none_or(|list| list.is_empty()) {
      self.start_terminal(root, window, cx);
    }
  }

  /// Starts a shell in `root` and remembers it as the project's last one.
  fn start_terminal(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<Terminal>> {
    let started = match Terminal::start(root.clone(), cx) {
      Ok(started) => started,
      Err(error) => {
        window.push_notification(Notification::error(format!("Could not start a shell: {error}")), cx);
        return None;
      }
    };
    let terminal = cx.new(|cx| Terminal::new(started, cx));
    self._terminal_subscriptions.push(cx.subscribe_in(&terminal, window, |this, terminal, _: &TerminalExited, window, cx| {
      this.close_terminal(terminal.clone(), window, cx);
    }));
    self.terminals.entry(root).or_default().push(terminal.clone());
    Some(terminal)
  }

  /// The shells of the open project and the position of the one the terminal panel is on.
  fn open_terminals(&self) -> (Vec<Entity<Terminal>>, usize) {
    let Some(root) = &self.projects.active else { return (Vec::new(), 0) };
    let list = self.terminals.get(root).cloned().unwrap_or_default();
    let active = self.active_terminal.get(root).copied().unwrap_or(0).min(list.len().saturating_sub(1));
    (list, active)
  }

  /// Gives the keyboard to the shell the terminal panel is on, once it is drawn.
  fn focus_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
    let (list, active) = self.open_terminals();
    if let Some(terminal) = list.get(active) {
      let handle = terminal.read(cx).focus_handle(cx);
      window.defer(cx, move |window, cx| handle.focus(window, cx));
    }
  }

  /// Shows the terminal panel and puts the keyboard in the shell.
  fn show_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.terminal_open = true;
    self.ensure_terminal(window, cx);
    self.focus_terminal(window, cx);
    cx.notify();
  }

  /// The title bar's terminal button and its shortcut: shows or hides the terminal panel. The shells keep running.
  pub fn toggle_terminal(&mut self, _: &ToggleTerminal, window: &mut Window, cx: &mut Context<Self>) {
    if self.terminal_open {
      self.hide_terminal(window, cx);
    } else {
      self.show_terminal(window, cx);
    }
  }

  /// Opens another shell in the open project and shows it.
  pub fn new_terminal(&mut self, _: &NewTerminal, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    if self.start_terminal(root.clone(), window, cx).is_none() {
      return;
    }
    let last = self.terminals.get(&root).map_or(0, |list| list.len().saturating_sub(1));
    self.active_terminal.insert(root, last);
    self.show_terminal(window, cx);
  }

  /// Shows two shells side by side, starting a second one if the project has only one.
  pub(crate) fn split_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    if self.terminals.get(&root).map_or(0, Vec::len) < 2 && self.start_terminal(root.clone(), window, cx).is_none() {
      return;
    }
    self.split_terminals.insert(root);
    self.show_terminal(window, cx);
  }

  /// Stops the shell the tab is on and takes it away. The last one is replaced by a fresh shell.
  pub(crate) fn kill_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (list, active) = self.open_terminals();
    if let Some(terminal) = list.get(active) {
      self.close_terminal(terminal.clone(), window, cx);
      self.focus_terminal(window, cx);
    }
  }

  /// Picks which shell the tab shows.
  pub(crate) fn select_terminal(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(root) = self.projects.active.clone() {
      self.active_terminal.insert(root, index);
    }
    self.focus_terminal(window, cx);
    cx.notify();
  }

  /// Hides the terminal panel; the shells keep running. The keyboard goes back to the window, so shortcuts keep working.
  pub(crate) fn hide_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.terminal_open = false;
    self.focus_handle.focus(window, cx);
    cx.notify();
  }

  /// A shell has the keyboard.
  fn terminal_focused(&self, window: &Window, cx: &App) -> bool {
    self.open_terminals().0.iter().any(|terminal| terminal.read(cx).focus_handle(cx).contains_focused(window, cx))
  }

  /// Removes a terminal, whose shell has exited or was killed; a project never stays without one.
  fn close_terminal(&mut self, terminal: Entity<Terminal>, window: &mut Window, cx: &mut Context<Self>) {
    for (root, list) in &mut self.terminals {
      let Some(position) = list.iter().position(|other| other == &terminal) else { continue };
      list.remove(position);
      let active = self.active_terminal.entry(root.clone()).or_insert(0);
      if *active >= position {
        *active = active.saturating_sub(1);
      }
      if list.len() < 2 {
        self.split_terminals.remove(root);
      }
    }
    self.ensure_terminal(window, cx);
    cx.notify();
  }

  /// Opens the Settings dialog.
  pub fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
    self.open_settings_page("General", window, cx);
  }

  /// Marks the account that serves `url` as having a permission problem, because the host refused its token: a warning shows by
  /// Settings until a check finds it fixed. `pull_requests` says which permission was refused.
  fn flag_account_issue(&mut self, url: &str, pull_requests: bool, cx: &mut Context<Self>) {
    self.settings.update(cx, |model, cx| {
      let Some(account) = model.accounts.items.iter_mut().find(|account| RepoRef::for_account(account, url).is_some()) else { return };
      let (action, permission) = if pull_requests {
        ("Opening a pull request", account.provider.pull_request_permission())
      } else {
        ("Pushing or pulling", account.provider.push_permission())
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
    window.open_dialog(cx, move |dialog, _, _| {
      // The dialog's own padding is removed so that the page list reaches the edges of the window.
      dialog.p_0().width(px(820.)).child(
        v_flex()
          .rounded_b_lg()
          .overflow_hidden()
          .child(Label::new("Settings").text_lg().font_weight(FontWeight::SEMIBOLD).px_6().py_4())
          .child(div().h(px(520.)).child(settings_view(fallback.clone(), projects.clone(), accounts.clone(), updates.clone(), page))),
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
      if self.terminals.is_empty() {
        self.ensure_terminal(window, cx);
        self.refresh_project();
      }
    }
    window.push_notification(Notification::info(message), cx);
    cx.notify();
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
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Add project").width(px(480.)).child(setup.clone()));
  }

  /// Writes the project list; a failure is logged, the window keeps working.
  fn persist_projects(&self) {
    if let Err(error) = self.projects.save(&ProjectList::default_path()) {
      eprintln!("Could not save the project list: {error}");
    }
  }

  /// Asks for a name and adds an empty category: a place to drop projects on in the project bar.
  pub(crate) fn create_category(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.ask_name("New category", "Category name", "", window, cx, move |this, name, _, cx| {
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

  /// Makes a category from a project, unnamed: the menu's way to start one without a second project.
  pub(crate) fn new_category(&mut self, project: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    self.ask_name("New category", "Category name", "", window, cx, move |this, name, _, cx| {
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
    self.ask_name("Rename category", "Category name", &initial, window, cx, move |this, name, _, cx| {
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
        .confirm()
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
    self.terminals.remove(path);
    self.views.remove(path);
    cx.notify();
  }

  /// Opens the dialog for a project's name, icon and color.
  pub(crate) fn open_project_settings(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
    let Some(project) = self.projects.projects.iter().find(|project| project.path == path).cloned() else { return };
    let settings = cx.new(|cx| ProjectSettings::new(&project, window, cx));
    self._dialog_subscription = Some(cx.subscribe_in(&settings, window, |this, _, chosen: &AppearanceChosen, window, cx| {
      this.projects.set_appearance(&chosen.path, &chosen.name, chosen.icon.as_deref(), chosen.color.as_deref());
      this.persist_projects();
      window.close_dialog(cx);
      cx.notify();
    }));
    let title = SharedString::from(format!("{} settings", project.name()));
    window.open_dialog(cx, move |dialog, _, _| dialog.title(title.clone()).width(px(460.)).child(settings.clone()));
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
    self.ask_name(format!("New {kind} in {place}"), placeholder, "", window, cx, move |this, name, window, cx| {
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
    window.open_dialog(cx, move |dialog, _, _| dialog.title(title.clone()).width(px(420.)).child(Input::new(&field)));
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
        .confirm()
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
                if message.contains(vekro_git::TOKEN_REFUSED) {
                  this.flag_account_issue(&remote_url, false, cx);
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
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Connect an account").width(px(540.)).child(form.clone()));
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
    window.open_dialog(cx, move |dialog, _, _| dialog.title("OpenCommit settings").width(px(560.)).child(form.clone()));
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
            if message.contains(vekro_git::TOKEN_REFUSED) {
              this.flag_account_issue(&remote_url, false, cx);
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
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move { vekro_git::history(&root, HISTORY_LIMIT) }
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
        if !known {
          match first {
            Some(id) => this.select_commit(id, cx),
            None => this.selected_commit = None,
          }
        }
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
        this.flag_account_issue(&url, true, cx);
      }
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Create a pull request").width(px(560.)).child(form.clone()));
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
    self.ask_name("New branch", "Branch name", &initial, window, cx, move |this, name, window, cx| {
      let root = this.projects.active.clone().ok_or("Open a project first.")?;
      vekro_git::create_branch(&root, name)?;
      this.refresh_project();
      window.push_notification(Notification::info(format!("Created and switched to {}.", name.trim())), cx);
      Ok(())
    });
  }

  /// Opens or closes the repository menu of the title bar; opening clears the filter and focuses it.
  fn set_projects_menu(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
    self.projects_menu_open = open;
    if open {
      self.project_filter.update(cx, |input, cx| input.set_value("", window, cx));
      let focus = self.project_filter.read(cx).focus_handle(cx);
      window.defer(cx, move |window, cx| focus.focus(window, cx));
    }
    cx.notify();
  }

  /// ⌘W: closes the open tab of the editor, then the others from the last to the first, and keeps the focus in the window so
  /// the shortcut keeps working. It never closes the window, and does nothing while a terminal has the keyboard.
  pub fn close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
    if !self.terminal_focused(window, cx) && self.editor_panel.update(cx, |editor, cx| editor.close_by_shortcut(cx)) {
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
          .child(Icon::empty().path("icons/folder-git-2.svg").size_5().text_color(theme.muted_foreground))
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
        let (filter, query, list) = {
          let this = workspace.read(cx);
          (this.project_filter.clone(), this.project_filter.read(cx).value().to_lowercase(), this.projects.clone())
        };
        let active = list.active.clone();
        let categories: Vec<(String, SharedString)> = list
          .categories
          .iter()
          .enumerate()
          .map(|(index, category)| (category.id.clone(), SharedString::from(category_label(index, category))))
          .collect();
        let matches = |project: &&Project| query.is_empty() || project.name().to_lowercase().contains(&query);
        let mut groups: Vec<(SharedString, Vec<&Project>)> = list
          .categories
          .iter()
          .enumerate()
          .map(|(index, category)| {
            (SharedString::from(category_label(index, category)), list.projects_in(Some(&category.id)).filter(matches).collect())
          })
          .collect();
        groups.push((SharedString::from("Other"), list.projects_in(None).filter(matches).collect()));
        groups.retain(|(_, projects)| !projects.is_empty());
        let named = groups.len() > 1 || !list.categories.is_empty();
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
            projects.into_iter().map(|project| project_list_row(&workspace, project, active.as_ref() == Some(&project.path), &categories, cx)),
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
        v_flex().w(MENU_WIDTH).gap_1().child(h_flex().gap_2().items_center().child(div().flex_1().child(Input::new(&filter))).child(add)).child(
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
            .when(count > 0, |row| row.child(div().px_1p5().rounded_full().bg(palette.selected).text_size(FONT_SIZE_SM).child(count.to_string()))),
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
    let theme = cx.theme().clone();
    let palette = colors(cx);
    if self.history.is_empty() {
      return Label::new("No commits yet").px_2().py_2().text_color(theme.muted_foreground).into_any_element();
    }
    let rows = self.history.iter().map(|commit| {
      let id = commit.id.clone();
      let selected = self.selected_commit.as_deref() == Some(commit.id.as_str());
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
    });
    v_flex().id("history-list").flex_1().min_h_0().pr_3().overflow_y_scrollbar().children(rows).into_any_element()
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
    v_flex().flex_1().min_h_0().gap_1().child(header).child(rows)
  }

  /// The repository summary and the folder tree.
  fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let selected = SidebarTab::ALL.iter().position(|tab| *tab == self.sidebar_tab).unwrap_or(0);
    let tabs = TabBar::new("sidebar-tabs")
      .underline()
      .selected_index(selected)
      .on_click(cx.listener(|this, index: &usize, _, cx| {
        if let Some(tab) = SidebarTab::ALL.get(*index) {
          this.sidebar_tab = *tab;
          cx.notify();
        }
      }))
      .children(SidebarTab::ALL.map(|tab| Tab::new().icon(tab.icon()).aria_label(tab.label())));
    let content = match self.sidebar_tab {
      SidebarTab::Explorer => self.render_explorer(cx).into_any_element(),
      SidebarTab::Git => match self.render_git(cx) {
        Some(git) => git.into_any_element(),
        None => Label::new("Not a Git repository").text_color(cx.theme().muted_foreground).px_2().into_any_element(),
      },
    };
    v_flex().size_full().bg(cx.theme().sidebar).child(div().px_3().pt_2().child(tabs)).child(v_flex().flex_1().min_h_0().pl_3().pt_3().child(content))
  }

  /// The terminal panel at the bottom of the window: a title row with the buttons, then the shell (or two of them side by side).
  fn render_terminal_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let (list, active) = self.open_terminals();
    let Some(current) = list.get(active).cloned() else {
      return Label::new("No project is open.").text_color(theme.muted_foreground).p_3().into_any_element();
    };
    let split = list.len() >= 2 && self.projects.active.as_ref().is_some_and(|root| self.split_terminals.contains(root));
    let shown: Vec<Entity<Terminal>> = if split { vec![current.clone(), list[(active + 1) % list.len()].clone()] } else { vec![current.clone()] };
    let workspace = cx.entity();
    let sessions: Vec<(usize, SharedString)> = list.iter().enumerate().map(|(index, terminal)| (index, terminal.read(cx).title())).collect();
    let button =
      |id: &'static str, icon: &'static str, tooltip: &'static str| Button::new(id).ghost().xsmall().icon(Icon::empty().path(icon)).tooltip(tooltip);
    let header = h_flex()
      .gap_1()
      .items_center()
      .px_3()
      .py_1()
      .border_b_1()
      .border_color(theme.border)
      .child(Icon::empty().path("icons/terminal.svg").size_4().text_color(theme.muted_foreground))
      .child(Label::new(current.read(cx).title()).flex_1().min_w_0().whitespace_nowrap().overflow_hidden().text_ellipsis())
      .child(
        button("terminal-new", "icons/plus.svg", "New terminal")
          .on_click(cx.listener(|this, _, window, cx| this.new_terminal(&NewTerminal, window, cx))),
      )
      .child(button("terminal-menu", "icons/chevron-down.svg", "More").dropdown_menu(move |menu, _, _| {
        let (new, split, kill) = (workspace.clone(), workspace.clone(), workspace.clone());
        let mut menu = menu;
        if sessions.len() > 1 {
          menu = menu.label("Sessions");
          for (index, title) in &sessions {
            let (workspace, index) = (workspace.clone(), *index);
            menu = menu.item(
              PopupMenuItem::new(format!("{} · {}", index + 1, title))
                .checked(index == active)
                .on_click(move |_, window, cx| workspace.update(cx, |this, cx| this.select_terminal(index, window, cx))),
            );
          }
          menu = menu.separator();
        }
        menu
          .item(
            PopupMenuItem::new("New terminal").on_click(move |_, window, cx| new.update(cx, |this, cx| this.new_terminal(&NewTerminal, window, cx))),
          )
          .item(PopupMenuItem::new("Split terminal").on_click(move |_, window, cx| split.update(cx, |this, cx| this.split_terminal(window, cx))))
          .separator()
          .item(PopupMenuItem::new("Kill terminal").on_click(move |_, window, cx| kill.update(cx, |this, cx| this.kill_terminal(window, cx))))
      }))
      .child(
        button("terminal-split", "icons/columns-2.svg", "Split terminal")
          .on_click(cx.listener(|this, _, window, cx| this.split_terminal(window, cx))),
      )
      .child(button("terminal-kill", "icons/trash.svg", "Kill terminal").on_click(cx.listener(|this, _, window, cx| this.kill_terminal(window, cx))))
      .child(
        button("terminal-close", "icons/x.svg", "Hide the terminal (shells keep running)")
          .on_click(cx.listener(|this, _, window, cx| this.hide_terminal(window, cx))),
      );
    v_flex()
      .size_full()
      .child(header)
      .child(h_flex().flex_1().min_h_0().children(shown.into_iter().enumerate().map(|(position, terminal)| {
        div().flex_1().min_w_0().h_full().when(position > 0, |pane| pane.border_l_1().border_color(theme.border)).child(terminal)
      })))
      .into_any_element()
  }
}

/// One project in a list (a category's list, the repository menu): its avatar and name; a click opens it and closes the list,
/// a right click opens the project's menu.
fn project_list_row(workspace: &Entity<Workspace>, project: &Project, selected: bool, categories: &[(String, SharedString)], cx: &App) -> AnyElement {
  let palette = colors(cx);
  let look = avatar_look(project, selected, cx);
  let (path, name) = (project.path.clone(), SharedString::from(project.name()));
  let (menu_workspace, menu_path, categories, current) = (workspace.clone(), project.path.clone(), categories.to_vec(), project.category.clone());
  h_flex()
    .id(SharedString::from(format!("project-row-{}", path.display())))
    .gap_2()
    .px_2()
    .py_1()
    .rounded_md()
    .items_center()
    .cursor_pointer()
    .when(selected, |row| row.bg(palette.selected))
    .hover(|row| row.bg(palette.hover))
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
    .on_click({
      let workspace = workspace.clone();
      move |_, window, cx| {
        workspace.update(cx, |this, cx| {
          this.projects_menu_open = false;
          this.select_project(path.clone(), window, cx);
        })
      }
    })
    .context_menu(move |menu, _, _| project_menu(menu, &menu_workspace, &menu_path, &categories, current.as_deref()))
    .into_any_element()
}

/// The right-click menu of a project avatar: its settings, the categories it can move to, and making a new one.
fn project_menu(
  menu: PopupMenu,
  workspace: &Entity<Workspace>,
  path: &Path,
  categories: &[(String, SharedString)],
  current: Option<&str>,
) -> PopupMenu {
  let (settings_workspace, settings_path) = (workspace.clone(), path.to_path_buf());
  let mut menu = menu.item(
    PopupMenuItem::new("Settings…")
      .on_click(move |_, window, cx| settings_workspace.update(cx, |this, cx| this.open_project_settings(&settings_path, window, cx))),
  );
  menu = menu.separator();
  if !categories.is_empty() {
    menu = menu.label("Move to");
    for (id, label) in categories {
      let (workspace, path, id_owned) = (workspace.clone(), path.to_path_buf(), id.clone());
      menu = menu.item(
        PopupMenuItem::new(label.clone())
          .checked(current == Some(id.as_str()))
          .on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.move_project(&path, Some(&id_owned), cx))),
      );
    }
    menu = menu.separator();
  }
  let (new_workspace, new_path) = (workspace.clone(), path.to_path_buf());
  menu = menu.item(
    PopupMenuItem::new("New category…")
      .on_click(move |_, window, cx| new_workspace.update(cx, |this, cx| this.new_category(new_path.clone(), window, cx))),
  );
  if current.is_some() {
    let (workspace, path) = (workspace.clone(), path.to_path_buf());
    menu = menu
      .item(PopupMenuItem::new("Remove from category").on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.move_project(&path, None, cx))));
  }
  menu
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
  palette: &crate::theme::Colors,
  on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
  let hover = palette.rail_hover;
  div()
    .id(id)
    .size_8()
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
    // The terminal panel sits under the editor, above the status bar, and can be dragged taller or shorter.
    let attention = self.settings.read(cx).accounts.items.iter().any(|account| account.issue.is_some());
    let commit_shown = self.sidebar_tab == SidebarTab::Git && self.git_view == GitView::History && self.selected_commit.is_some();
    let change_shown = self.sidebar_tab == SidebarTab::Git && self.git_view == GitView::Changes && self.selected_change.is_some();
    let editor_area: AnyElement = if commit_shown {
      self.commit_view.clone().into_any_element()
    } else if change_shown {
      self.change_view.clone().into_any_element()
    } else {
      self.dock.clone().into_any_element()
    };
    let main_area = if self.terminal_open {
      v_resizable("editor-terminal-split")
        .child(resizable_panel().child(editor_area))
        .child(resizable_panel().size(TERMINAL_HEIGHT).size_range(TERMINAL_MIN_HEIGHT..TERMINAL_MAX_HEIGHT).child(self.render_terminal_area(cx)))
        .into_any_element()
    } else {
      editor_area
    };
    v_flex()
      .id("workspace")
      .key_context("Workspace")
      .track_focus(&self.focus_handle)
      .on_action(cx.listener(Self::add_project))
      .on_action(cx.listener(Self::open_settings))
      .on_action(cx.listener(Self::new_terminal))
      .on_action(cx.listener(Self::toggle_terminal))
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
            .child(rail_button("title-add", Icon::new(IconName::Plus), "Add project", false, &palette, |_, window, cx| {
              window.dispatch_action(Box::new(AddProject), cx)
            }))
            .child(div().flex_1())
            .child(rail_button(
              "title-terminal",
              Icon::empty().path("icons/terminal.svg"),
              "Terminal",
              self.terminal_open,
              &palette,
              |_, window, cx| window.dispatch_action(Box::new(ToggleTerminal), cx),
            ))
            .child(
              div()
                .relative()
                .child(rail_button("title-settings", Icon::new(IconName::Settings), "Settings", false, &palette, |_, window, cx| {
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
                }),
            ),
        ),
      )
      .child(
        h_flex().flex_1().min_h_0().child(
          h_resizable("sidebar-split")
            .child(resizable_panel().size(SIDEBAR_WIDTH).size_range(SIDEBAR_MIN_WIDTH..SIDEBAR_MAX_WIDTH).child(self.render_sidebar(cx)))
            // The status bar belongs to the editor and the terminals: the sidebar runs down to the bottom of the window.
            .child(
              resizable_panel().child(
                v_flex()
                  .size_full()
                  .when(self.sidebar_tab == SidebarTab::Git, |column| column.children(self.render_git_toolbar(cx)))
                  .child(div().flex_1().min_h_0().child(main_area))
                  .child(self.render_status_bar(cx)),
              ),
            ),
        ),
      )
  }
}
