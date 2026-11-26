//! Terminal tabs, AI agent tabs and pane layout.

use super::*;

impl Workspace {
  /// The tabs of the open project's shells.
  pub(super) fn arrangement(&self) -> Option<&Arrangement<Entity<Terminal>>> {
    self.projects.active.as_ref().and_then(|root| self.terminal_tabs.get(root))
  }

  pub(super) fn project_has_terminal_activity(&self, root: &Path, cx: &App) -> bool {
    self.terminal_tabs.get(root).is_some_and(|tabs| tabs.panes().iter().any(|terminal| terminal.read(cx).has_foreground_job()))
  }

  /// Starts a shell in the open project if it has none: a project is not left without one.
  pub(super) fn ensure_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    // A shell cannot start in a folder that is gone; the project's page says so instead.
    if !root.is_dir() {
      return;
    }
    if self.terminal_tabs.get(&root).is_none_or(Arrangement::is_empty) {
      if let Some(terminal) = self.start_terminal(root.clone(), window, cx) {
        self.terminal_tabs.entry(root).or_insert_with(Arrangement::new).add_tab(terminal);
      }
    }
  }

  /// Starts a shell in `root`; the caller puts it in a tab or a pane.
  pub(super) fn start_terminal(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<Terminal>> {
    self.start_program(root, None, "a shell", window, cx)
  }

  /// Starts `program` in `root` (the login shell for `None`); `what` names it in the notification when it cannot start.
  pub(super) fn start_program(
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
        window.push_notification(crate::failure(format!("Could not start {what}: {error}")), cx);
        return None;
      }
    };
    let terminal = cx.new(|cx| Terminal::new(started, cx));
    self._terminal_subscriptions.push(cx.subscribe_in(&terminal, window, |this, terminal, _: &TerminalExited, window, cx| {
      this.close_pane(terminal.clone(), window, cx);
    }));
    self._terminal_subscriptions.push(cx.subscribe(&terminal, |_, _, _: &TerminalAttentionChanged, cx| cx.notify()));
    self._terminal_subscriptions.push(cx.subscribe(&terminal, |_, _, _: &TerminalActivityChanged, cx| cx.notify()));
    Some(terminal)
  }

  /// The shell that has the keyboard, else the first of the shown tab.
  pub(super) fn focused_terminal(&self, window: &Window, cx: &App) -> Option<Entity<Terminal>> {
    let panes = self.arrangement()?.current()?.panes();
    panes
      .iter()
      .find(|terminal| terminal.read(cx).focus_handle(cx).contains_focused(window, cx))
      .or(panes.first())
      .map(|terminal| (*terminal).clone())
  }

  /// A shell has the keyboard.
  pub(super) fn terminal_focused(&self, window: &Window, cx: &App) -> bool {
    self.arrangement().is_some_and(|tabs| tabs.panes().iter().any(|terminal| terminal.read(cx).focus_handle(cx).contains_focused(window, cx)))
  }

  /// Gives the keyboard to a shell, once it is drawn.
  pub(super) fn focus_pane(&self, terminal: &Entity<Terminal>, window: &mut Window, cx: &mut Context<Self>) {
    let handle = terminal.read(cx).focus_handle(cx);
    window.defer(cx, move |window, cx| handle.focus(window, cx));
  }

  /// Gives the keyboard to the shell that should have it.
  pub(super) fn focus_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(terminal) = self.focused_terminal(window, cx) {
      self.focus_pane(&terminal, window, cx);
    }
  }

  /// Shows the terminals in the main area and puts the keyboard in the shell.
  pub(super) fn show_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.leave_project_settings();
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
    self.leave_project_settings();
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

  /// Replaces a pane's shell with a new one in the same place and folder, for a shell that hangs or is in a broken state.
  pub(crate) fn restart_pane(&mut self, target: Entity<Terminal>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let directory = target.read(cx).directory().filter(|directory| directory.is_dir()).unwrap_or_else(|| root.clone());
    let Some(terminal) = self.start_terminal(directory, window, cx) else { return };
    let replaced = self.terminal_tabs.get_mut(&root).is_some_and(|tabs| tabs.replace(&target, terminal.clone()));
    if !replaced {
      window.push_notification(crate::failure("Could not restart this pane: it is not a shell of this project."), cx);
      return;
    }
    self.focus_pane(&terminal, window, cx);
    cx.notify();
  }

  /// Restarts the Codex daemon in the background and says how it went.
  pub(crate) fn restart_codex_daemon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    window.push_notification(Notification::info("Restarting the Codex daemon…"), cx);
    let work = cx.background_executor().spawn(async { crate::daemon::restart_codex_daemon() });
    cx.spawn_in(window, async move |_, cx| {
      let result = work.await;
      let _ = cx.update(|window, cx| match result {
        Ok(()) => window.push_notification(Notification::success("The Codex daemon was restarted. Start Codex again in the pane."), cx),
        Err(message) => window.push_notification(crate::failure(message), cx),
      });
    })
    .detach();
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
    self.leave_project_settings();
    self.terminal_open = false;
    self.agent_open = false;
    self.focus_handle.focus(window, cx);
    cx.notify();
  }

  /// The agent sessions of the open project.
  pub(super) fn agents(&self) -> Option<&AgentTabs<Entity<Terminal>>> {
    self.projects.active.as_ref().and_then(|root| self.agent_tabs.get(root))
  }

  /// A full-width view is showing: the terminals, the AI view or the project settings. The sidebar, the editor and the status bar
  /// are hidden then.
  pub(super) fn full_area_open(&self) -> bool {
    self.terminal_open || self.agent_open || self.project_settings.is_some() || self.app_settings.is_some()
  }

  /// Gives the keyboard to the shown agent session, once it is drawn.
  pub(super) fn focus_agent(&self, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(tab) = self.agents().and_then(AgentTabs::current) {
      self.focus_pane(&tab.session, window, cx);
    }
  }

  /// The activity bar's AI button and its shortcut: the AI view takes the main area, or gives it back to the editor.
  pub fn toggle_agents(&mut self, _: &ToggleAgents, window: &mut Window, cx: &mut Context<Self>) {
    if self.agents_missing(window, cx) {
      return;
    }
    if self.agent_open {
      self.hide_terminal(window, cx);
    } else {
      self.leave_project_settings();
      self.terminal_open = false;
      self.agent_open = true;
      self.focus_agent(window, cx);
      cx.notify();
    }
  }

  /// Whether the AI Agent extension is missing: then the view of the extensions opens, with a note, since that is where it is installed.
  fn agents_missing(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
    if !extensions::enabled_agents(cx).is_empty() {
      return false;
    }
    self.sidebar_tab = SidebarTab::Extensions;
    self.extensions_panel.update(cx, |panel, cx| panel.ensure_loaded(cx));
    window.push_notification(Notification::info("Install the AI Agent extension to use agents."), cx);
    cx.notify();
    true
  }

  pub fn new_codex(&mut self, _: &NewCodex, window: &mut Window, cx: &mut Context<Self>) {
    self.new_agent(AgentKind::Codex, window, cx);
  }

  pub fn new_claude(&mut self, _: &NewClaude, window: &mut Window, cx: &mut Context<Self>) {
    self.new_agent(AgentKind::Claude, window, cx);
  }

  /// Starts an agent in a tab of its own in the open project and shows the AI view. It runs in the user's login shell, so
  /// the PATH of their shell finds it.
  pub(super) fn new_agent(&mut self, kind: AgentKind, window: &mut Window, cx: &mut Context<Self>) {
    if self.agents_missing(window, cx) {
      return;
    }
    let Some(root) = self.projects.active.clone() else { return };
    let shell = std::env::var("SHELL").ok().filter(|shell| !shell.is_empty()).unwrap_or_else(|| "/bin/zsh".to_owned());
    let program = (shell, vec!["-l".to_owned(), "-i".to_owned(), "-c".to_owned(), kind.shell_line()]);
    let Some(terminal) = self.start_program(root.clone(), Some(program), kind.label(), window, cx) else { return };
    self.agent_tabs.entry(root).or_insert_with(AgentTabs::new).add(kind, terminal.clone());
    self.leave_project_settings();
    self.terminal_open = false;
    self.agent_open = true;
    self.focus_pane(&terminal, window, cx);
    cx.notify();
  }

  /// `⇧⌘M` in the AI view or in the terminals: opens the project list for the shown session or the focused pane.
  pub fn mention_projects(&mut self, _: &MentionProjects, window: &mut Window, cx: &mut Context<Self>) {
    let target = if self.agent_open {
      self.agents().and_then(AgentTabs::current).map(|tab| tab.session.clone())
    } else if self.terminal_open {
      self.focused_terminal(window, cx)
    } else {
      None
    };
    if let Some(terminal) = target {
      self.pick_projects_to_mention(terminal, window, cx);
    }
  }

  /// Opens the project list: the ones ticked are written into the prompt of the agent as paths.
  pub(super) fn pick_projects_to_mention(&mut self, terminal: Entity<Terminal>, window: &mut Window, cx: &mut Context<Self>) {
    let choices: Vec<MentionChoice> = self
      .projects
      .projects
      .iter()
      .map(|project| MentionChoice { path: project.path.clone(), name: project.name().into(), folder: display_path(&project.path).into() })
      .collect();
    if choices.is_empty() {
      window.push_notification(Notification::info("Add a project to mention it in an agent."), cx);
      return;
    }
    let picker = cx.new(|cx| MentionPicker::new(choices, window, cx));
    self._dialog_subscription = Some(cx.subscribe_in(&picker, window, move |this, _, chosen: &ProjectsChosen, window, cx| {
      let text = ws_terminal::dropped_paths(chosen.0.iter().map(PathBuf::as_path));
      terminal.update(cx, |terminal, cx| terminal.insert_text(&text, cx));
      window.close_dialog(cx);
      this.focus_pane(&terminal, window, cx);
    }));
    let shown = picker.clone();
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Mention projects").width(px(440.)).p_5().child(shown.clone()));
    let focus = picker.read(cx).focus_handle(cx);
    window.defer(cx, move |window, cx| focus.focus(window, cx));
  }

  pub(super) fn select_agent_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(root) = self.projects.active.clone() {
      if let Some(tabs) = self.agent_tabs.get_mut(&root) {
        tabs.select(index);
      }
    }
    self.focus_agent(window, cx);
    cx.notify();
  }

  /// Closes an agent session: the agent stops.
  pub(super) fn close_agent_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
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
}
