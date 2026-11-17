use crate::{
  components::{avatar_look, display_path},
  editor::{ActiveTab, EditorPanel},
  explorer,
  project_manager::ProjectManager,
  project_settings::{AppearanceChosen, ProjectSettings},
  project_setup::{ProjectReady, ProjectSetup},
  settings::{init_settings, settings_view, AddAllProjectsRequested, SettingsGlobal, SettingsModel},
  terminal::{Terminal, TerminalExited},
  theme::{apply_theme, colors, FONT_SIZE, FONT_SIZE_SM},
  AddProject, NewTerminal, OpenSettings, SaveFile,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    dock::{panel_handle, DockArea, DockLayout, DockSkin},
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    label::Label,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    resizable::{h_resizable, resizable_panel},
    scroll::{ScrollableElement as _, Scrollbar, ScrollbarAxis},
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
  time::Duration,
};
use vekro_git::Summary;
use vekro_storage::{Category, Preferences, Project, ProjectList};

/// Height of the top band: the traffic lights and the title.
pub const TOOLBAR_HEIGHT: Pixels = px(40.);

/// How long the pointer must rest on a file in the Explorer before it is prepared for opening, so that moving past
/// files costs nothing.
const HOVER_PREPARE_DELAY: Duration = Duration::from_millis(120);

/// How often the Git summary and the folder tree are read again while the window is open.
const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// Width of the icon rail on the far left.
const RAIL_WIDTH: Pixels = px(60.);

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
  Terminal,
}

impl SidebarTab {
  const ALL: [SidebarTab; 3] = [SidebarTab::Explorer, SidebarTab::Git, SidebarTab::Terminal];

  fn icon(self) -> Icon {
    match self {
      SidebarTab::Explorer => Icon::empty().path("icons/folder-tree.svg"),
      SidebarTab::Git => Icon::empty().path("icons/git-branch.svg"),
      SidebarTab::Terminal => Icon::empty().path("icons/terminal.svg"),
    }
  }

  fn label(self) -> &'static str {
    match self {
      SidebarTab::Explorer => "Explorer",
      SidebarTab::Git => "Git",
      SidebarTab::Terminal => "Terminal",
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

/// One window: the rail and panel on the left and the open project on the right.
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
  /// Which of a project's shells the Terminal tab shows.
  active_terminal: HashMap<PathBuf, usize>,
  /// The projects whose Terminal tab shows two shells side by side.
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
      _commit_subscriptions: vec![
        // The commit button is enabled once there is a summary.
        cx.subscribe(&commit_summary, |_, _, _: &InputEvent, cx| cx.notify()),
      ],
      _settings_subscriptions: vec![
        // Any change to a setting is saved and applied in every window.
        cx.observe_in(&settings, window, |_, settings, window, cx| {
          let preferences = settings.read(cx).preferences.clone();
          if let Err(error) = preferences.save(&Preferences::default_path()) {
            eprintln!("Could not save the settings: {error}");
          }
          apply_theme(preferences.theme, preferences.code_font_size, window, cx);
          cx.notify();
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
      _active_tab: cx.subscribe(&editor_panel, |this, _, shown: &ActiveTab, cx| {
        match shown {
          ActiveTab::File(path) => {
            this.selected_entry = Some(path.clone());
            this.selected_change = None;
          }
          ActiveTab::Diff(name) => {
            this.selected_entry = None;
            this.selected_change = Some(PathBuf::from(name));
          }
          ActiveTab::Nothing => {
            this.selected_entry = None;
            this.selected_change = None;
          }
        }
        cx.notify();
      }),
      _activation: cx.observe_window_activation(window, |this, window, cx| {
        if window.is_window_active() {
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

  /// The shells of the open project and the position of the one the Terminal tab is on.
  fn open_terminals(&self) -> (Vec<Entity<Terminal>>, usize) {
    let Some(root) = &self.projects.active else { return (Vec::new(), 0) };
    let list = self.terminals.get(root).cloned().unwrap_or_default();
    let active = self.active_terminal.get(root).copied().unwrap_or(0).min(list.len().saturating_sub(1));
    (list, active)
  }

  /// Gives the keyboard to the shell the Terminal tab is on, once the tab is drawn.
  fn focus_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
    let (list, active) = self.open_terminals();
    if let Some(terminal) = list.get(active) {
      let handle = terminal.read(cx).focus_handle(cx);
      window.defer(cx, move |window, cx| handle.focus(window, cx));
    }
  }

  /// Switches to the Terminal tab and puts the keyboard in the shell.
  fn show_terminal_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.sidebar_tab = SidebarTab::Terminal;
    self.focus_terminal(window, cx);
    cx.notify();
  }

  /// Opens another shell in the open project and shows it.
  pub fn new_terminal(&mut self, _: &NewTerminal, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    if self.start_terminal(root.clone(), window, cx).is_none() {
      return;
    }
    let last = self.terminals.get(&root).map_or(0, |list| list.len().saturating_sub(1));
    self.active_terminal.insert(root, last);
    self.show_terminal_tab(window, cx);
  }

  /// Shows two shells side by side, starting a second one if the project has only one.
  pub(crate) fn split_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    if self.terminals.get(&root).map_or(0, Vec::len) < 2 && self.start_terminal(root.clone(), window, cx).is_none() {
      return;
    }
    self.split_terminals.insert(root);
    self.show_terminal_tab(window, cx);
  }

  /// Stops the shell the tab is on and takes it away. The last one is replaced by a fresh shell.
  pub(crate) fn kill_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (list, active) = self.open_terminals();
    if let Some(terminal) = list.get(active) {
      self.close_terminal(terminal.clone(), window, cx);
      self.focus_terminal(window, cx);
    }
  }

  /// Stops the shell at `index` of the open project and takes it away.
  pub(crate) fn kill_terminal_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
    let (list, _) = self.open_terminals();
    if let Some(terminal) = list.get(index) {
      self.close_terminal(terminal.clone(), window, cx);
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

  /// Leaves the Terminal tab; the shells keep running.
  pub(crate) fn hide_terminal_tab(&mut self, cx: &mut Context<Self>) {
    self.sidebar_tab = SidebarTab::Explorer;
    cx.notify();
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
    let fallback = self.new_project_dir(cx);
    let workspace = cx.entity();
    let projects = cx.new(|cx| ProjectManager::new(workspace, cx));
    window.open_dialog(cx, move |dialog, _, _| {
      // The dialog's own padding is removed so that the page list reaches the edges of the window.
      dialog.p_0().width(px(820.)).child(
        v_flex()
          .rounded_b_lg()
          .overflow_hidden()
          .child(Label::new("Settings").text_lg().font_weight(FontWeight::SEMIBOLD).px_6().py_4())
          .child(div().h(px(520.)).child(settings_view(fallback.clone(), projects.clone()))),
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
    let setup = cx.new(|cx| ProjectSetup::new(location, window, cx));
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

  pub(crate) fn toggle_category(&mut self, id: &str, cx: &mut Context<Self>) {
    self.projects.toggle_category(id);
    self.persist_projects();
    cx.notify();
  }

  pub(crate) fn move_project(&mut self, path: &Path, category: Option<&str>, cx: &mut Context<Self>) {
    self.projects.set_category(path, category);
    self.persist_projects();
    cx.notify();
  }

  /// A project was dropped on another one: they share a category now, a new unnamed one if there was none.
  fn group_projects(&mut self, dragged: &Path, target: &Path, cx: &mut Context<Self>) {
    if self.projects.group(dragged, target).is_some() {
      self.persist_projects();
      cx.notify();
    }
  }

  /// The projects and categories the rail shows.
  pub(crate) fn project_list(&self) -> &ProjectList {
    &self.projects
  }

  /// Moves a project up or down among the projects of its category.
  pub(crate) fn move_project_by(&mut self, path: &Path, delta: isize, cx: &mut Context<Self>) {
    self.projects.move_project_by(path, delta);
    self.persist_projects();
    cx.notify();
  }

  /// Moves a category up or down in the rail.
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
    let after = self.projects.active.clone();
    let view = after.as_ref().and_then(|root| self.views.remove(root)).unwrap_or_default();
    self.expanded = view.expanded;
    self.selected_entry = view.selected_entry;
    self.selected_change = view.selected_change;
    self.editor_panel.update(cx, |editor, cx| editor.switch_project(before, after, cx));
    self.refresh_project();
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

  /// Opens the diff of a changed file in the editor area.
  fn open_change(&mut self, file: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let name = file.display().to_string();
    let result = vekro_git::file_diff(&root, &file).and_then(|patch| self.editor_panel.update(cx, |editor, cx| editor.open_diff(name, &patch, cx)));
    match result {
      Ok(()) => {
        self.selected_change = Some(file);
        self.selected_entry = None;
      }
      Err(message) => window.push_notification(Notification::error(message), cx),
    }
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
          let name = file.display().to_string();
          self.editor_panel.update(cx, |editor, cx| editor.forget(&path, &name, cx));
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
    let work = cx.background_executor().spawn({
      let (root, files) = (root.clone(), files.clone());
      async move { vekro_git::commit(&root, &files, &summary, &description) }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.committing = false;
        match result {
          Ok(id) => {
            this.commit_summary.update(cx, |input, cx| input.set_value("", window, cx));
            this.commit_description.update(cx, |input, cx| input.set_value("", window, cx));
            this.excluded.clear();
            // The diff of a file that was just committed has nothing left to show.
            if let Some(open) = this.selected_change.clone().filter(|open| files.contains(open)) {
              if let Some(path) = vekro_git::workdir_file(&root, &open) {
                this.editor_panel.update(cx, |editor, cx| editor.forget(&path, &open.display().to_string(), cx));
              }
              this.selected_change = None;
            }
            this.refresh_project();
            window.push_notification(Notification::info(format!("Committed {id}.")), cx);
          }
          Err(message) => window.push_notification(Notification::error(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
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

  /// One project's avatar. It can be dragged onto another project or a category, and a right click opens its menu.
  fn rail_project(&self, project: &Project, grouped: bool, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let path = project.path.clone();
    let name = SharedString::from(project.name());
    let look = avatar_look(project, self.projects.active.as_ref() == Some(&project.path), cx);
    let selected = self.projects.active.as_ref() == Some(&project.path);
    // Inside a category the block is already the hover color, so an uncolored avatar needs a lighter one.
    let (fill, text) = (look.fill, look.text);
    let hover = if grouped && !selected && project.color.is_none() { palette.rail_item_hover } else { look.hover };
    let drag_over = if grouped { palette.rail_item_hover } else { palette.rail_hover };
    let workspace = cx.entity();
    let categories: Vec<(String, SharedString)> = self
      .projects
      .categories
      .iter()
      .enumerate()
      .map(|(index, category)| (category.id.clone(), SharedString::from(category_label(index, category))))
      .collect();
    let current = project.category.clone();
    let (drag_label, drag_fill, drag_text) = (look.letter.clone(), fill.unwrap_or(palette.rail_hover), text);
    div()
      .id(SharedString::from(format!("rail-{}", path.display())))
      .size_9()
      .flex()
      .flex_none()
      .items_center()
      .justify_center()
      .rounded_lg()
      .cursor_pointer()
      .text_base()
      .font_weight(FontWeight::MEDIUM)
      .text_color(text)
      .when_some(fill, |avatar, fill| avatar.bg(fill))
      .hover(move |avatar| avatar.bg(hover))
      .drag_over::<DraggedProject>(move |style, _, _, _| style.bg(drag_over))
      .when_some(look.icon.clone(), |avatar, icon| avatar.child(Icon::empty().path(icon).size_5()))
      .when(look.icon.is_none(), |avatar| avatar.child(look.letter.clone()))
      .tooltip(move |window, cx| Tooltip::new(name.clone()).build(window, cx))
      .on_drag(DraggedProject(path.clone()), move |_, _, _, cx| {
        cx.new(|_| DragPreview { label: drag_label.clone().into(), fill: drag_fill, text: drag_text })
      })
      .on_drop(cx.listener({
        let target = path.clone();
        move |this, dragged: &DraggedProject, _, cx| this.group_projects(&dragged.0, &target, cx)
      }))
      .on_click(cx.listener({
        let path = path.clone();
        move |this, _, window, cx| this.select_project(path.clone(), window, cx)
      }))
      .context_menu(move |menu, _, _| {
        let (settings_workspace, settings_path) = (workspace.clone(), path.clone());
        let mut menu = menu.item(
          PopupMenuItem::new("Settings…")
            .on_click(move |_, window, cx| settings_workspace.update(cx, |this, cx| this.open_project_settings(&settings_path, window, cx))),
        );
        menu = menu.separator();
        if !categories.is_empty() {
          menu = menu.label("Move to");
          for (id, label) in &categories {
            let (workspace, path, id_owned) = (workspace.clone(), path.clone(), id.clone());
            let checked = current.as_deref() == Some(id.as_str());
            menu = menu.item(
              PopupMenuItem::new(label.clone())
                .checked(checked)
                .on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.move_project(&path, Some(&id_owned), cx))),
            );
          }
          menu = menu.separator();
        }
        let (new_workspace, new_path) = (workspace.clone(), path.clone());
        menu = menu.item(
          PopupMenuItem::new("New category…")
            .on_click(move |_, window, cx| new_workspace.update(cx, |this, cx| this.new_category(new_path.clone(), window, cx))),
        );
        if current.is_some() {
          let (workspace, path) = (workspace.clone(), path.clone());
          menu = menu.item(
            PopupMenuItem::new("Remove from category").on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.move_project(&path, None, cx))),
          );
        }
        menu
      })
  }

  /// A category in the rail: one tile that folds and unfolds it, and its projects below while it is unfolded.
  fn rail_category(&self, index: usize, category: &Category, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let members: Vec<&Project> = self.projects.projects_in(Some(&category.id)).collect();
    let label = SharedString::from(category_label(index, category));
    let has_active = members.iter().any(|project| self.projects.active.as_ref() == Some(&project.path));
    let folded = category.collapsed;
    let shows_initials = folded && !members.is_empty();
    let workspace = cx.entity();
    let tile = div()
      .id(SharedString::from(format!("rail-category-{}", category.id)))
      .size_9()
      .flex()
      .flex_none()
      .items_center()
      .justify_center()
      .rounded_lg()
      .cursor_pointer()
      .text_color(palette.rail_text)
      .when(folded, |tile| tile.bg(palette.rail_hover))
      .when(folded && has_active, |tile| tile.border_1().border_color(palette.accent))
      .hover(move |tile| tile.bg(palette.rail_item_hover))
      .drag_over::<DraggedProject>(move |style, _, _, _| style.bg(palette.rail_item_hover))
      .tooltip({
        let label = label.clone();
        move |window, cx| Tooltip::new(label.clone()).build(window, cx)
      })
      .on_drop(cx.listener({
        let id = category.id.clone();
        move |this, dragged: &DraggedProject, _, cx| this.move_project(&dragged.0, Some(&id), cx)
      }))
      .on_click(cx.listener({
        let id = category.id.clone();
        move |this, _, _, cx| this.toggle_category(&id, cx)
      }))
      .when(shows_initials, |tile| {
        // Discord's folded folder: the first four projects as small initials.
        let cells: Vec<AnyElement> = members
          .iter()
          .take(4)
          .map(|project| {
            let active = self.projects.active.as_ref() == Some(&project.path);
            div()
              .size_4()
              .flex()
              .flex_none()
              .items_center()
              .justify_center()
              .text_xs()
              .when(active, |cell| cell.text_color(palette.accent))
              .child(initial(&project.name()))
              .into_any_element()
          })
          .collect();
        let mut rows = Vec::new();
        let mut cells = cells.into_iter();
        while let Some(first) = cells.next() {
          rows.push(h_flex().child(first).children(cells.next()));
        }
        tile.child(v_flex().items_center().justify_center().children(rows))
      })
      .when(!shows_initials, |tile| tile.child(Icon::empty().path(if folded { "icons/folder.svg" } else { "icons/folder-open.svg" }).size_5()))
      .context_menu({
        let id = category.id.clone();
        move |menu, _, _| {
          let (rename_workspace, rename_id) = (workspace.clone(), id.clone());
          let (delete_workspace, delete_id) = (workspace.clone(), id.clone());
          menu
            .item(
              PopupMenuItem::new("Rename…")
                .on_click(move |_, window, cx| rename_workspace.update(cx, |this, cx| this.rename_category(rename_id.clone(), window, cx))),
            )
            .item(
              PopupMenuItem::new("Delete category")
                .on_click(move |_, _, cx| delete_workspace.update(cx, |this, cx| this.delete_category(&delete_id, cx))),
            )
        }
      });
    v_flex()
      .items_center()
      .gap_1()
      .when(!folded, |group| group.bg(palette.rail_hover).rounded_lg().py_1())
      .child(tile)
      .when(!folded, |group| group.children(members.into_iter().map(|project| self.rail_project(project, true, cx))))
  }

  fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let loose: Vec<AnyElement> = self.projects.projects_in(None).map(|project| self.rail_project(project, false, cx).into_any_element()).collect();
    let groups: Vec<AnyElement> =
      self.projects.categories.iter().enumerate().map(|(index, category)| self.rail_category(index, category, cx).into_any_element()).collect();
    v_flex()
      .w(RAIL_WIDTH)
      .h_full()
      .flex_none()
      .items_center()
      .pt_3()
      .pb_1p5()
      .gap_2()
      .bg(palette.rail)
      .border_r_1()
      .border_color(palette.border)
      .child(
        v_flex()
          .id("rail-projects")
          .flex_1()
          .min_h_0()
          .items_center()
          .gap_2()
          .overflow_y_scroll()
          .children(loose)
          .children(groups)
          .child(rail_button("rail-add", IconName::Plus, "Add project", &palette, |_, window, cx| window.dispatch_action(Box::new(AddProject), cx))),
      )
      .child(rail_button("rail-settings", IconName::Settings, "Settings", &palette, |_, window, cx| {
        window.dispatch_action(Box::new(OpenSettings), cx)
      }))
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
                move |this, _, window, cx| this.open_change(file.clone(), window, cx)
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
      .child(Textarea::new(&self.commit_description).h(px(88.)))
      .child(
        Button::new("commit")
          .primary()
          .w_full()
          .label(label)
          .disabled(chosen == 0 || !summary_filled || self.committing)
          .on_click(cx.listener(|this, _, window, cx| this.commit_changes(window, cx))),
      );
    Some(
      v_flex()
        .size_full()
        .gap_1()
        .child(
          h_flex()
            .pl_2()
            .pr_5()
            .py_1()
            .gap_2()
            .items_center()
            .child(Icon::empty().path("icons/git-branch.svg").size_4().text_color(theme.muted_foreground))
            .child(Label::new(git.branch.clone()).flex_1())
            .child(Icon::new(IconName::ChevronDown).size_4().text_color(theme.muted_foreground)),
        )
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
        .child(commit_box),
    )
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
      .on_click(cx.listener(|this, index: &usize, window, cx| {
        if let Some(tab) = SidebarTab::ALL.get(*index) {
          this.sidebar_tab = *tab;
          if *tab == SidebarTab::Terminal {
            this.focus_terminal(window, cx);
          }
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
      SidebarTab::Terminal => self.render_terminal_sessions(cx).into_any_element(),
    };
    v_flex().size_full().bg(cx.theme().sidebar).child(div().px_3().pt_2().child(tabs)).child(v_flex().flex_1().min_h_0().pl_3().pt_3().child(content))
  }

  /// The Terminal tab of the sidebar: the shells of the project as a list. The terminal itself is drawn in the main area.
  fn render_terminal_sessions(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let (list, active) = self.open_terminals();
    let split = list.len() >= 2 && self.projects.active.as_ref().is_some_and(|root| self.split_terminals.contains(root));
    let partner = (active + 1) % list.len().max(1);
    let rows = list.iter().enumerate().map(|(index, terminal)| {
      let shown_beside = split && index == partner;
      h_flex()
        .id(("session", index))
        .gap_2()
        .pl_2()
        .pr_1()
        .py_1()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .when(index == active, |row| row.bg(palette.selected))
        .when(shown_beside, |row| row.bg(palette.hover))
        .hover(|row| row.bg(palette.hover))
        .child(Icon::empty().path("icons/terminal.svg").size_4().flex_none().text_color(theme.muted_foreground))
        .child(Label::new(terminal.read(cx).title()).flex_1().min_w_0().whitespace_nowrap().overflow_hidden().text_ellipsis())
        .child(Label::new(format!("{}", index + 1)).text_color(theme.muted_foreground).text_size(FONT_SIZE_SM))
        .child(Button::new(("session-kill", index)).ghost().xsmall().icon(Icon::empty().path("icons/x.svg")).tooltip("Kill terminal").on_click(
          cx.listener(move |this, _, window, cx| {
            // The click must not also select the session.
            cx.stop_propagation();
            this.kill_terminal_at(index, window, cx);
          }),
        ))
        .on_click(cx.listener(move |this, _, window, cx| this.select_terminal(index, window, cx)))
    });
    v_flex()
      .size_full()
      .gap_1()
      .child(
        h_flex().gap_1().items_center().pl_2().pr_3().child(Label::new("Sessions").font_weight(FontWeight::SEMIBOLD).flex_1()).child(
          Button::new("session-new")
            .ghost()
            .xsmall()
            .icon(Icon::empty().path("icons/plus.svg"))
            .tooltip("New terminal")
            .on_click(cx.listener(|this, _, window, cx| this.new_terminal(&NewTerminal, window, cx))),
        ),
      )
      .child(v_flex().id("sessions").flex_1().min_h_0().pr_3().overflow_y_scrollbar().children(rows))
  }

  /// The terminal in the main area: a title row with the buttons, then the shell (or two of them side by side).
  fn render_terminal_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let (list, active) = self.open_terminals();
    let Some(current) = list.get(active).cloned() else {
      return Label::new("No project is open.").text_color(theme.muted_foreground).p_3().into_any_element();
    };
    let split = list.len() >= 2 && self.projects.active.as_ref().is_some_and(|root| self.split_terminals.contains(root));
    let shown: Vec<Entity<Terminal>> = if split { vec![current.clone(), list[(active + 1) % list.len()].clone()] } else { vec![current.clone()] };
    let workspace = cx.entity();
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
        button("terminal-close", "icons/x.svg", "Back to the editor (shells keep running)")
          .on_click(cx.listener(|this, _, _, cx| this.hide_terminal_tab(cx))),
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

/// What a category is called in a menu or a tooltip: its name, or its place in the rail while it has none.
fn category_label(index: usize, category: &Category) -> String {
  if category.name.is_empty() {
    format!("Category {}", index + 1)
  } else {
    category.name.clone()
  }
}

/// A project avatar being dragged in the rail.
#[derive(Clone)]
struct DraggedProject(PathBuf);

/// What follows the pointer while a project is dragged.
struct DragPreview {
  label: SharedString,
  fill: Hsla,
  text: Hsla,
}

impl Render for DragPreview {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    div()
      .size_9()
      .flex()
      .items_center()
      .justify_center()
      .rounded_lg()
      .text_base()
      .font_weight(FontWeight::MEDIUM)
      .bg(self.fill)
      .text_color(self.text)
      .opacity(0.9)
      .child(self.label.clone())
  }
}

/// The first letter of a project's name, in capitals: its avatar.
fn initial(name: &str) -> String {
  name.chars().take(1).flat_map(char::to_uppercase).collect()
}

/// A square icon button for the project rail. The Kit's `Button` fixes its icon at a small size, so this draws the
/// icon itself at `size_5`.
fn rail_button(
  id: &'static str,
  icon: IconName,
  tooltip: &'static str,
  palette: &crate::theme::Colors,
  on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
  let hover = palette.rail_hover;
  div()
    .id(id)
    .size_9()
    .flex()
    .flex_none()
    .items_center()
    .justify_center()
    .rounded_lg()
    .cursor_pointer()
    .text_color(palette.rail_text)
    .hover(move |button| button.bg(hover))
    .child(Icon::new(icon).size_5())
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
    // The terminal tab of the sidebar puts the terminal in the main area, where the editor otherwise is.
    let main_area =
      if self.sidebar_tab == SidebarTab::Terminal { self.render_terminal_area(cx).into_any_element() } else { self.dock.clone().into_any_element() };
    v_flex()
      .id("workspace")
      .key_context("Workspace")
      .track_focus(&self.focus_handle)
      .on_action(cx.listener(Self::add_project))
      .on_action(cx.listener(Self::open_settings))
      .on_action(cx.listener(Self::new_terminal))
      .on_action(cx.listener(Self::save_file))
      .size_full()
      .bg(theme.background)
      .text_color(theme.foreground)
      .text_size(FONT_SIZE)
      .child(TitleBar::new().h(TOOLBAR_HEIGHT).pl(px(96.)).bg(palette.rail).border_b_1().border_color(palette.border))
      .child(
        h_flex().flex_1().min_h_0().child(self.render_rail(cx)).child(
          h_resizable("sidebar-split")
            .child(resizable_panel().size(SIDEBAR_WIDTH).size_range(SIDEBAR_MIN_WIDTH..SIDEBAR_MAX_WIDTH).child(self.render_sidebar(cx)))
            // The status bar belongs to the editor and the terminals: the rail and the sidebar run down to the bottom of the window.
            .child(resizable_panel().child(v_flex().size_full().child(div().flex_1().min_h_0().child(main_area)).child(self.render_status_bar(cx)))),
        ),
      )
  }
}
