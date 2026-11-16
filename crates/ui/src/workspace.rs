use crate::{
  components::{display_path, selected_style},
  editor::EditorPanel,
  explorer,
  project_setup::{ProjectReady, ProjectSetup},
  settings::{init_settings, settings_view, AddAllProjectsRequested, SettingsGlobal, SettingsModel},
  terminal::{NewTerminalRequested, Terminal, TerminalExited},
  theme::{apply_theme, colors, FONT_SIZE, FONT_SIZE_SM},
  AddProject, NewTerminal, OpenSettings, SaveFile,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    dock::{panel_handle, DockArea, DockLayout, DockPlacement, DockSkin},
    h_flex,
    input::{Input, InputEvent, InputState},
    label::Label,
    menu::{ContextMenuExt as _, PopupMenuItem},
    notification::Notification,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement as _,
    status_bar::StatusBar,
    tab::{Tab, TabBar},
    tooltip::Tooltip,
    v_flex, ActiveTheme, Icon, IconName, Sizable as _, TitleBar, WindowExt as _,
  },
  *,
};
use std::{
  collections::{HashMap, HashSet},
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
const RAIL_WIDTH: Pixels = px(56.);

/// Width of the sidebar when the window opens.
const SIDEBAR_WIDTH: Pixels = px(280.);

/// How narrow and how wide the sidebar can be dragged.
const SIDEBAR_MIN_WIDTH: Pixels = px(200.);
const SIDEBAR_MAX_WIDTH: Pixels = px(480.);

/// Height of the bottom dock when the window opens.
const BOTTOM_DOCK_HEIGHT: Pixels = px(260.);

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
  _entry_subscription: Option<Subscription>,
  /// Whether the Explorer shows its tree or only its header.
  explorer_open: bool,
  _settings_subscriptions: Vec<Subscription>,
  _appearance: Subscription,
  _activation: Subscription,
  /// Repaints the window when the editor changes: the status bar shows the cursor.
  _editor: Subscription,
  _refresh_task: Task<()>,
  dock: Entity<DockArea>,
  editor_panel: Entity<EditorPanel>,
  /// The shells of every project that has been opened, kept alive while another project is shown.
  terminals: HashMap<PathBuf, Vec<Entity<Terminal>>>,
  _terminal_subscriptions: Vec<Subscription>,
}

impl Workspace {
  pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    // The first window loads the settings; later windows share them.
    let settings = match cx.try_global::<SettingsGlobal>() {
      Some(global) => global.0.clone(),
      None => init_settings(cx),
    };
    apply_theme(settings.read(cx).preferences.theme, window, cx);
    let (dock, _skin) = DockSkin::dock_area("workspace-dock", Some(1), window, cx);
    let editor_panel = cx.new(|cx| EditorPanel::new(window, cx));
    dock.update(cx, |area, cx| {
      area.set_center(DockLayout::h_split().child(DockLayout::tabs().panel_view(panel_handle(editor_panel.clone()), cx), None), window, cx);
    });
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
      _entry_subscription: None,
      explorer_open: true,
      _settings_subscriptions: vec![
        // Any change to a setting is saved and applied in every window.
        cx.observe_in(&settings, window, |_, settings, window, cx| {
          let preferences = settings.read(cx).preferences.clone();
          if let Err(error) = preferences.save(&Preferences::default_path()) {
            eprintln!("Could not save the settings: {error}");
          }
          apply_theme(preferences.theme, window, cx);
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
      _terminal_subscriptions: Vec::new(),
      _appearance: cx.observe_window_appearance(window, |this, window, cx| apply_theme(this.settings.read(cx).preferences.theme, window, cx)),
      // Changes made in another app (an editor, GitHub Desktop, a terminal) show up as soon as the window is back.
      _editor: cx.observe(&editor_panel, |_, _, cx| cx.notify()),
      _activation: cx.observe_window_activation(window, |this, window, cx| {
        if window.is_window_active() {
          this.refresh_in_background(cx);
        }
      }),
      _refresh_task: cx.spawn(async move |this, cx| loop {
        cx.background_executor().timer(REFRESH_INTERVAL).await;
        if this.update(cx, |this, cx| this.refresh_in_background(cx)).is_err() {
          break;
        }
      }),
    };
    this.refresh_project();
    this.show_terminals(window, cx);
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

  /// Puts the active project's shells in the bottom dock, starting one if the project has none yet.
  fn show_terminals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    if self.terminals.get(&root).is_none_or(|list| list.is_empty()) {
      self.start_terminal(root.clone(), window, cx);
    }
    for terminal in self.terminals.get(&root).cloned().unwrap_or_default() {
      self.dock.update(cx, |area, cx| area.add_panel_view(panel_handle(terminal), DockPlacement::Bottom, Some(BOTTOM_DOCK_HEIGHT), window, cx));
    }
  }

  /// Takes the active project's shells off the screen without stopping them.
  fn hide_terminals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    for terminal in self.terminals.get(&root).cloned().unwrap_or_default() {
      self.dock.update(cx, |area, cx| area.remove_panel(terminal, window, cx));
    }
  }

  /// Starts a shell in `root` and remembers it; the caller puts it on screen.
  fn start_terminal(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let started = match Terminal::start(root.clone(), cx) {
      Ok(started) => started,
      Err(error) => {
        window.push_notification(Notification::error(format!("Could not start a shell: {error}")), cx);
        return;
      }
    };
    let terminal = cx.new(|cx| Terminal::new(started, cx));
    self._terminal_subscriptions.push(cx.subscribe_in(&terminal, window, |this, terminal, _: &TerminalExited, window, cx| {
      this.close_terminal(terminal.clone(), window, cx);
    }));
    self._terminal_subscriptions.push(cx.subscribe_in(&terminal, window, |this, _, _: &NewTerminalRequested, window, cx| {
      this.new_terminal(&NewTerminal, window, cx);
    }));
    self.terminals.entry(root).or_default().push(terminal);
  }

  /// Opens another shell next to the others in the active project.
  pub fn new_terminal(&mut self, _: &NewTerminal, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let before = self.terminals.get(&root).map_or(0, Vec::len);
    self.start_terminal(root.clone(), window, cx);
    let Some(terminal) = self.terminals.get(&root).filter(|list| list.len() > before).and_then(|list| list.last().cloned()) else { return };
    self.dock.update(cx, |area, cx| area.add_panel_view(panel_handle(terminal.clone()), DockPlacement::Bottom, Some(BOTTOM_DOCK_HEIGHT), window, cx));
    terminal.update(cx, |terminal, cx| terminal.focus(window, cx));
  }

  /// Removes a terminal whose shell has exited; a project never stays without one.
  fn close_terminal(&mut self, terminal: Entity<Terminal>, window: &mut Window, cx: &mut Context<Self>) {
    self.dock.update(cx, |area, cx| area.remove_panel(terminal.clone(), window, cx));
    for list in self.terminals.values_mut() {
      list.retain(|other| other != &terminal);
    }
    if self.projects.active.as_ref().is_some_and(|root| self.terminals.get(root).is_none_or(|list| list.is_empty())) {
      self.show_terminals(window, cx);
    }
  }

  /// Opens the Settings dialog.
  pub fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
    let fallback = self.new_project_dir(cx);
    window
      .open_dialog(cx, move |dialog, _, _| dialog.title("Settings").width(px(720.)).child(div().h(px(500.)).child(settings_view(fallback.clone()))));
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
        self.show_terminals(window, cx);
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

  /// Asks for the name of a new category and puts `project`, if given, in it.
  fn new_category(&mut self, project: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
    self.ask_name("New category", "Category name", "", window, cx, move |this, name, _, cx| {
      if !this.projects.add_category(name) {
        return Err("Enter a category name that is not used yet.".into());
      }
      if let Some(path) = &project {
        this.projects.set_category(path, Some(name.trim()));
      }
      this.persist_projects();
      cx.notify();
      Ok(())
    });
  }

  fn rename_category(&mut self, old: String, window: &mut Window, cx: &mut Context<Self>) {
    let initial = old.clone();
    self.ask_name("Rename category", "Category name", &initial, window, cx, move |this, name, _, cx| {
      if !this.projects.rename_category(&old, name) {
        return Err("Enter a category name that is not used yet.".into());
      }
      this.persist_projects();
      cx.notify();
      Ok(())
    });
  }

  fn delete_category(&mut self, name: &str, cx: &mut Context<Self>) {
    self.projects.remove_category(name);
    self.persist_projects();
    cx.notify();
  }

  fn toggle_category(&mut self, name: &str, cx: &mut Context<Self>) {
    self.projects.toggle_category(name);
    self.persist_projects();
    cx.notify();
  }

  fn move_project(&mut self, path: &Path, category: Option<&str>, cx: &mut Context<Self>) {
    self.projects.set_category(path, category);
    self.persist_projects();
    cx.notify();
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
    // The terminals must leave while the old project is still the open one.
    self.hide_terminals(window, cx);
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
    let after = self.projects.active.clone();
    let view = after.as_ref().and_then(|root| self.views.remove(root)).unwrap_or_default();
    self.expanded = view.expanded;
    self.selected_entry = view.selected_entry;
    self.selected_change = view.selected_change;
    self.editor_panel.update(cx, |editor, cx| editor.switch_project(before, after, cx));
    self.refresh_project();
    self.show_terminals(window, cx);
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
    self._entry_subscription = Some(cx.subscribe_in(&input, window, move |this, input, event: &InputEvent, window, cx| {
      if let InputEvent::PressEnter { .. } = event {
        let name = input.read(cx).value().to_string();
        match submit(this, &name, window, cx) {
          Ok(()) => window.close_all_dialogs(cx),
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
    self.explorer_open = true;
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

  /// One project's avatar. A right click offers the categories to put it in.
  fn rail_project(&self, project: &Project, cx: &mut Context<Self>) -> impl IntoElement {
    let selected_style = selected_style(cx);
    let path = project.path.clone();
    let name = project.name();
    let selected = self.projects.active.as_ref() == Some(&project.path);
    let letter = initial(&name);
    let workspace = cx.entity();
    let categories: Vec<SharedString> = self.projects.categories.iter().map(|category| SharedString::from(category.name.clone())).collect();
    let current = project.category.clone();
    let owner = path.clone();
    div()
      .id(SharedString::from(format!("rail-item-{}", path.display())))
      .child(
        Button::new(SharedString::from(format!("rail-{}", path.display())))
          .label(letter)
          .w_10()
          .h_10()
          .rounded_lg()
          .when(selected, |button| button.custom(selected_style))
          .when(!selected, |button| button.ghost())
          .tooltip(name)
          .on_click(cx.listener(move |this, _, window, cx| this.select_project(path.clone(), window, cx))),
      )
      .context_menu(move |menu, _, _| {
        let mut menu = menu;
        if !categories.is_empty() {
          menu = menu.label("Move to");
          for category in &categories {
            let (workspace, owner, target) = (workspace.clone(), owner.clone(), category.clone());
            let checked = current.as_deref() == Some(category.as_ref());
            menu = menu.item(
              PopupMenuItem::new(category.clone())
                .checked(checked)
                .on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.move_project(&owner, Some(&target), cx))),
            );
          }
          menu = menu.separator();
        }
        let (new_workspace, new_owner) = (workspace.clone(), owner.clone());
        menu = menu.item(
          PopupMenuItem::new("New category…")
            .on_click(move |_, window, cx| new_workspace.update(cx, |this, cx| this.new_category(Some(new_owner.clone()), window, cx))),
        );
        if current.is_some() {
          let (workspace, owner) = (workspace.clone(), owner.clone());
          menu = menu.item(
            PopupMenuItem::new("Remove from category").on_click(move |_, _, cx| workspace.update(cx, |this, cx| this.move_project(&owner, None, cx))),
          );
        }
        menu
      })
  }

  /// A category in the rail: one tile that folds and unfolds it, and its projects below while it is unfolded.
  fn rail_category(&self, category: &Category, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let members: Vec<&Project> = self.projects.projects_in(Some(&category.name)).collect();
    let name = SharedString::from(category.name.clone());
    let has_active = members.iter().any(|project| self.projects.active.as_ref() == Some(&project.path));
    let folded = category.collapsed;
    let workspace = cx.entity();
    let tile = div()
      .id(SharedString::from(format!("rail-category-{}", category.name)))
      .size_10()
      .flex()
      .flex_none()
      .items_center()
      .justify_center()
      .rounded_lg()
      .cursor_pointer()
      .text_color(palette.rail_text)
      .when(folded, |tile| tile.bg(palette.rail_hover))
      .when(folded && has_active, |tile| tile.border_1().border_color(palette.accent))
      .hover(move |tile| tile.bg(palette.rail_hover))
      .tooltip({
        let name = name.clone();
        move |window, cx| Tooltip::new(name.clone()).build(window, cx)
      })
      .on_click(cx.listener({
        let name = category.name.clone();
        move |this, _, _, cx| this.toggle_category(&name, cx)
      }))
      .when(folded && !members.is_empty(), |tile| {
        // Discord's folded folder: the first four projects as small initials.
        tile.child(div().size_full().flex().flex_wrap().items_center().justify_center().children(members.iter().take(4).map(|project| {
          let active = self.projects.active.as_ref() == Some(&project.path);
          div()
            .size_4()
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .when(active, |cell| cell.text_color(palette.accent))
            .child(initial(&project.name()))
        })))
      })
      .when(!(folded && !members.is_empty()), |tile| {
        tile.child(Icon::empty().path(if folded { "icons/folder.svg" } else { "icons/folder-open.svg" }).size_5())
      })
      .context_menu({
        let name = category.name.clone();
        move |menu, _, _| {
          let (rename_workspace, rename_name) = (workspace.clone(), name.clone());
          let (delete_workspace, delete_name) = (workspace.clone(), name.clone());
          menu
            .item(
              PopupMenuItem::new("Rename…")
                .on_click(move |_, window, cx| rename_workspace.update(cx, |this, cx| this.rename_category(rename_name.clone(), window, cx))),
            )
            .item(
              PopupMenuItem::new("Delete category")
                .on_click(move |_, _, cx| delete_workspace.update(cx, |this, cx| this.delete_category(&delete_name, cx))),
            )
        }
      });
    v_flex()
      .items_center()
      .gap_1()
      .when(!folded, |group| group.bg(palette.rail_hover).rounded_xl().p_1())
      .child(tile)
      .when(!folded, |group| group.children(members.into_iter().map(|project| self.rail_project(project, cx))))
  }

  fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let loose: Vec<AnyElement> = self.projects.projects_in(None).map(|project| self.rail_project(project, cx).into_any_element()).collect();
    let groups: Vec<AnyElement> = self.projects.categories.iter().map(|category| self.rail_category(category, cx).into_any_element()).collect();
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
      .border_color(palette.rail_border)
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
    let changes_list = v_flex().id("git-changes").max_h(px(170.)).overflow_y_scrollbar().children(git.changes.iter().map(|change| {
      let file = change.path.clone();
      let selected = self.selected_change.as_ref() == Some(&change.path);
      let name = change.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
      let folder = change.path.parent().map(|parent| parent.display().to_string()).filter(|parent| !parent.is_empty());
      h_flex()
        .id(SharedString::from(format!("change-{}", change.path.display())))
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded_md()
        .items_center()
        .cursor_pointer()
        .when(selected, |row| row.bg(palette.selected))
        .hover(|row| row.bg(palette.hover))
        .child(Label::new(change.kind.mark()).w_4().text_color(kind_color(change.kind, &palette)))
        .child(Label::new(name).flex_none())
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
        .on_click(cx.listener(move |this, _, window, cx| this.open_change(file.clone(), window, cx)))
    }));
    Some(
      v_flex()
        .gap_1()
        .child(
          h_flex()
            .px_2()
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
        .when(git.changed() > 0, |section| section.child(changes_list)),
    )
  }

  fn render_explorer(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let rows = self.rows.iter().map(|row| {
      let path = row.path.clone();
      let selected = self.selected_entry.as_ref() == Some(&row.path);
      let open = self.expanded.contains(&row.path);
      let icon = if row.is_dir { IconName::FolderClosed } else { IconName::File };
      h_flex()
        .id(SharedString::from(format!("entry-{}", row.path.display())))
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
        .child(Icon::new(icon).size_4().text_color(theme.muted_foreground))
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
    });
    let name = self
      .projects
      .active
      .as_ref()
      .and_then(|path| path.file_name())
      .map(|name| SharedString::from(name.to_string_lossy().into_owned()))
      .unwrap_or_else(|| "Vekro".into());
    let open = self.explorer_open;
    let action =
      |id: &'static str, icon: &'static str, tooltip: &'static str| Button::new(id).ghost().xsmall().icon(Icon::empty().path(icon)).tooltip(tooltip);
    let header = h_flex()
      .gap_1()
      .items_center()
      .child(
        h_flex()
          .id("explorer-title")
          .flex_1()
          .min_w_0()
          .gap_1()
          .items_center()
          .cursor_pointer()
          .child(Icon::new(if open { IconName::ChevronDown } else { IconName::ChevronRight }).size_4().text_color(theme.muted_foreground))
          .child(Label::new(name).font_weight(FontWeight::SEMIBOLD).whitespace_nowrap().text_ellipsis())
          .on_click(cx.listener(|this, _, _, cx| {
            this.explorer_open = !this.explorer_open;
            cx.notify();
          })),
      )
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
    v_flex()
      .flex_1()
      .min_h_0()
      .gap_1()
      .child(header)
      .when(open, |section| section.child(v_flex().id("explorer-rows").flex_1().min_h_0().overflow_y_scrollbar().children(rows)))
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
    v_flex().size_full().bg(cx.theme().sidebar).child(div().px_3().pt_2().child(tabs)).child(v_flex().flex_1().min_h_0().p_3().child(content))
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
    .size_10()
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
      .child(TitleBar::new().h(TOOLBAR_HEIGHT).pl(px(96.)).bg(palette.rail).border_b_1().border_color(palette.rail_border))
      .child(
        h_flex().flex_1().min_h_0().child(self.render_rail(cx)).child(
          h_resizable("sidebar-split")
            .child(resizable_panel().size(SIDEBAR_WIDTH).size_range(SIDEBAR_MIN_WIDTH..SIDEBAR_MAX_WIDTH).child(self.render_sidebar(cx)))
            .child(resizable_panel().child(self.dock.clone())),
        ),
      )
      .child(self.render_status_bar(cx))
  }
}
