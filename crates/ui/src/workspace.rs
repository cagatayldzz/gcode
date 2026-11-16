use crate::{
  components::selected_style,
  editor::EditorPanel,
  explorer,
  panels::ExplorerPanel,
  project_setup::{ProjectReady, ProjectSetup},
  settings::{PreferencesChanged, Settings},
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
    label::Label,
    notification::Notification,
    scroll::ScrollableElement as _,
    status_bar::StatusBar,
    v_flex, ActiveTheme, Icon, IconName, Sizable as _, TitleBar, WindowExt as _,
  },
  *,
};
use std::{
  collections::{HashMap, HashSet},
  path::PathBuf,
  time::Duration,
};
use vekro_git::Summary;
use vekro_storage::{Preferences, ProjectList};

/// Height of the top band: the traffic lights and the title.
pub const TOOLBAR_HEIGHT: Pixels = px(44.);

/// How often the Git summary and the folder tree are read again while the window is open.
const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// Width of the icon rail on the far left.
const RAIL_WIDTH: Pixels = px(56.);

/// Width of the left dock when the window opens.
const LEFT_DOCK_WIDTH: Pixels = px(280.);

/// Height of the bottom dock when the window opens.
const BOTTOM_DOCK_HEIGHT: Pixels = px(260.);

/// Indentation of each tree level.
const INDENT: Pixels = px(16.);

/// One window: the rail and panel on the left and the open project on the right.
pub struct Workspace {
  projects: ProjectList,
  preferences: Preferences,
  git: Option<Summary>,
  expanded: HashSet<PathBuf>,
  rows: Vec<explorer::Row>,
  selected_file: Option<PathBuf>,
  /// The changed file whose diff is open, relative to the repository.
  selected_change: Option<PathBuf>,
  focus_handle: FocusHandle,
  _setup_subscription: Option<Subscription>,
  _settings_subscription: Option<Subscription>,
  _appearance: Subscription,
  _activation: Subscription,
  _refresh_task: Task<()>,
  dock: Entity<DockArea>,
  /// Told to redraw whenever the workspace changes, because it draws the workspace's data.
  explorer_panel: Entity<ExplorerPanel>,
  editor_panel: Entity<EditorPanel>,
  /// The shells of every project that has been opened, kept alive while another project is shown.
  terminals: HashMap<PathBuf, Vec<Entity<Terminal>>>,
  _terminal_subscriptions: Vec<Subscription>,
}

impl Workspace {
  pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let preferences = Preferences::load(&Preferences::default_path());
    apply_theme(preferences.theme, window, cx);
    let (dock, _skin) = DockSkin::dock_area("workspace-dock", Some(1), window, cx);
    let workspace = cx.weak_entity();
    let explorer_panel = cx.new(|cx| ExplorerPanel::new(workspace, cx));
    let editor_panel = cx.new(EditorPanel::new);
    dock.update(cx, |area, cx| {
      area.set_center(DockLayout::h_split().child(DockLayout::tabs().panel_view(panel_handle(editor_panel.clone()), cx), None), window, cx);
      area.set_dock(DockPlacement::Left, DockLayout::tabs().panel_view(panel_handle(explorer_panel.clone()), cx), window, cx);
      area.set_dock_size(DockPlacement::Left, LEFT_DOCK_WIDTH, window, cx);
    });
    let mut this = Self {
      projects: ProjectList::load(&ProjectList::default_path()),
      preferences,
      git: None,
      expanded: HashSet::new(),
      rows: Vec::new(),
      selected_file: None,
      selected_change: None,
      focus_handle: cx.focus_handle(),
      _setup_subscription: None,
      _settings_subscription: None,
      dock,
      explorer_panel,
      editor_panel,
      terminals: HashMap::new(),
      _terminal_subscriptions: Vec::new(),
      _appearance: cx.observe_window_appearance(window, |this, window, cx| apply_theme(this.preferences.theme, window, cx)),
      // Changes made in another app (an editor, GitHub Desktop, a terminal) show up as soon as the window is back.
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
  fn new_project_dir(&self) -> PathBuf {
    self
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

  /// Redraws the window and the sidebar panel, which draws this workspace's data from inside the dock.
  fn changed(&mut self, cx: &mut Context<Self>) {
    cx.notify();
    self.explorer_panel.update(cx, |_, cx| cx.notify());
  }

  /// Opens the Settings dialog.
  pub fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
    let settings = cx.new(|_| Settings::new(self.preferences.clone(), self.new_project_dir()));
    self._settings_subscription = Some(cx.subscribe_in(&settings, window, |this, _, event: &PreferencesChanged, window, cx| {
      this.preferences = event.0.clone();
      if let Err(error) = this.preferences.save(&Preferences::default_path()) {
        eprintln!("Could not save the settings: {error}");
      }
      apply_theme(this.preferences.theme, window, cx);
      this.changed(cx);
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Settings").width(px(520.)).child(settings.clone()));
  }

  /// Opens the dialog that creates, clones or imports a project.
  pub fn add_project(&mut self, _: &AddProject, window: &mut Window, cx: &mut Context<Self>) {
    let location = self.new_project_dir();
    let setup = cx.new(|cx| ProjectSetup::new(location, window, cx));
    self._setup_subscription = Some(cx.subscribe_in(&setup, window, |this, _, event: &ProjectReady, window, cx| {
      window.close_all_dialogs(cx);
      this.projects.add(event.0.clone());
      this.project_changed(window, cx);
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Add project").width(px(480.)).child(setup.clone()));
  }

  fn select_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    self.projects.active = Some(path);
    self.project_changed(window, cx);
  }

  fn project_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    // The shells of the project being left keep running; they just leave the screen.
    self.hide_terminals(window, cx);
    if let Err(error) = self.projects.save(&ProjectList::default_path()) {
      eprintln!("Could not save the project list: {error}");
    }
    self.expanded.clear();
    self.selected_file = None;
    self.selected_change = None;
    self.refresh_project();
    self.show_terminals(window, cx);
    self.changed(cx);
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
          this.changed(cx);
        }
      });
    })
    .detach();
  }

  fn toggle_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    if !self.expanded.remove(&path) {
      self.expanded.insert(path);
    }
    self.refresh_project();
    self.changed(cx);
  }

  /// Opens a file from the Explorer in the editor.
  fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let result = self.editor_panel.update(cx, |editor, cx| editor.open_file(path.clone(), window, cx));
    match result {
      Ok(()) => {
        self.selected_file = Some(path);
        self.selected_change = None;
      }
      Err(message) => window.push_notification(Notification::error(message), cx),
    }
    self.changed(cx);
  }

  /// Opens the diff of a changed file in the editor area.
  fn open_change(&mut self, file: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let name = file.display().to_string();
    let result = vekro_git::file_diff(&root, &file).and_then(|patch| self.editor_panel.update(cx, |editor, cx| editor.open_diff(name, &patch, cx)));
    match result {
      Ok(()) => {
        self.selected_change = Some(file);
        self.selected_file = None;
      }
      Err(message) => window.push_notification(Notification::error(message), cx),
    }
    self.changed(cx);
  }

  /// Saves the file open in the editor.
  pub fn save_file(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
    match self.editor_panel.update(cx, |editor, cx| editor.save(cx)) {
      Ok(_) => self.changed(cx),
      Err(error) => window.push_notification(Notification::error(format!("Could not save: {error}")), cx),
    }
    // The file's own change shows up in the Git list without waiting for the next read.
    self.refresh_in_background(cx);
  }

  /// The thin bar at the bottom of the window: the branch and changes on the left, the open file on the right.
  fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let file = self.editor_panel.read(cx).status();
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
    if let Some(project) = self.projects.active.as_ref().map(|path| path.display().to_string()) {
      bar = bar.child(project);
    }
    if let Some(file) = file {
      bar = bar.right(file);
    }
    bar
  }

  fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let selected_style = selected_style(cx);
    let avatars = self.projects.projects.iter().map(|project| {
      let path = project.path.clone();
      let name = project.name();
      let selected = self.projects.active.as_ref() == Some(&project.path);
      let letter: String = name.chars().take(1).flat_map(char::to_uppercase).collect();
      Button::new(SharedString::from(format!("rail-{}", project.path.display())))
        .label(letter)
        .w_10()
        .h_10()
        .rounded_lg()
        .when(selected, |button| button.custom(selected_style))
        .when(!selected, |button| button.ghost())
        .tooltip(name)
        .on_click(cx.listener(move |this, _, window, cx| this.select_project(path.clone(), window, cx)))
    });
    v_flex()
      .w(RAIL_WIDTH)
      .h_full()
      .flex_none()
      .items_center()
      .py_3()
      .gap_2()
      .bg(theme.sidebar)
      .border_r_1()
      .border_color(theme.sidebar_border)
      .child(
        v_flex().id("rail-projects").flex_1().min_h_0().items_center().gap_2().overflow_y_scroll().children(avatars).child(
          Button::new("rail-add")
            .icon(IconName::Plus)
            .ghost()
            .w_10()
            .h_10()
            .rounded_lg()
            .tooltip("Add project")
            .on_click(|_, window, cx| window.dispatch_action(Box::new(AddProject), cx)),
        ),
      )
      .child(
        Button::new("rail-settings")
          .icon(IconName::Settings)
          .ghost()
          .w_10()
          .h_10()
          .rounded_lg()
          .tooltip("Settings")
          .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenSettings), cx)),
      )
  }

  fn render_projects(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let rows = self.projects.projects.iter().map(|project| {
      let path = project.path.clone();
      let selected = self.projects.active.as_ref() == Some(&project.path);
      h_flex()
        .id(SharedString::from(format!("project-{}", project.path.display())))
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded_md()
        .items_center()
        .cursor_pointer()
        .when(selected, |row| row.bg(palette.selected))
        .hover(|row| row.bg(palette.hover))
        .child(Icon::new(IconName::FolderClosed).size_4().text_color(theme.muted_foreground))
        .child(Label::new(project.name()))
        .on_click(cx.listener(move |this, _, window, cx| this.select_project(path.clone(), window, cx)))
    });
    v_flex()
      .gap_1()
      .child(h_flex().justify_between().items_center().px_2().child(Label::new("Projects").font_weight(FontWeight::MEDIUM)).child(
        Button::new("add-project").icon(IconName::Plus).ghost().small().on_click(|_, window, cx| window.dispatch_action(Box::new(AddProject), cx)),
      ))
      .children(rows)
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
        .child(Label::new("Git").font_weight(FontWeight::MEDIUM).px_2())
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
      let selected = self.selected_file.as_ref() == Some(&row.path);
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
        .on_click(cx.listener(move |this, _, window, cx| {
          if this.rows.iter().any(|row| row.path == path && row.is_dir) {
            this.toggle_folder(path.clone(), cx);
          } else {
            this.open_file(path.clone(), window, cx);
          }
        }))
    });
    let name = self.projects.active.as_ref().and_then(|path| path.file_name()).map(|name| name.to_string_lossy().into_owned());
    v_flex()
      .flex_1()
      .min_h_0()
      .gap_1()
      .child(Label::new("Explorer").font_weight(FontWeight::MEDIUM).px_2())
      .when_some(name, |section, name| {
        section.child(
          h_flex()
            .px_2()
            .py_1()
            .gap_1p5()
            .items_center()
            .child(div().size_4().flex_none())
            .child(Icon::new(IconName::FolderOpen).size_4().text_color(theme.muted_foreground))
            .child(Label::new(name)),
        )
      })
      .child(v_flex().id("explorer-rows").flex_1().min_h_0().overflow_y_scrollbar().children(rows))
  }

  /// The projects, repository summary and folder tree, drawn by the Explorer panel.
  pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
    let theme = cx.theme().clone();
    let divider = || div().h_px().w_full().bg(theme.sidebar_border);
    v_flex()
      .size_full()
      .p_3()
      .gap_3()
      .bg(theme.sidebar)
      .child(self.render_projects(cx))
      .child(divider())
      .when_some(self.render_git(cx), |panel, git| panel.child(git).child(divider()))
      .child(self.render_explorer(cx))
      .into_any_element()
  }
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
    let title = self
      .projects
      .active
      .as_ref()
      .and_then(|path| path.file_name())
      .map(|name| name.to_string_lossy().into_owned())
      .unwrap_or_else(|| "Vekro".into());
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
      .child(
        TitleBar::new()
          .h(TOOLBAR_HEIGHT)
          .pl(px(96.))
          .bg(palette.surface)
          .border_b_1()
          .border_color(palette.border)
          .child(Label::new(title).font_weight(FontWeight::SEMIBOLD)),
      )
      .child(h_flex().flex_1().min_h_0().child(self.render_rail(cx)).child(div().flex_1().min_w_0().h_full().child(self.dock.clone())))
      .child(self.render_status_bar(cx))
  }
}
