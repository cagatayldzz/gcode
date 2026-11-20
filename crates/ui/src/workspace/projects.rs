//! The project list, project switching and refreshing.

use super::*;

impl Workspace {
  /// Adds the repositories directly inside the projects folder that are not listed yet, and says how many.
  pub(super) fn add_projects_from_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
  pub(super) fn remember_window(&mut self, window: &Window, cx: &mut Context<Self>) {
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
      // The new project gets a tab in the title bar, and opens in the Explorer, not in the terminal that was showing.
      this.switch_project(window, cx, move |projects| {
        projects.add(path.clone());
        projects.set_favorite(&path, true);
      });
      this.sidebar_tab = SidebarTab::Explorer;
      this.hide_terminal(window, cx);
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.p_0().width(px(560.)).child(setup.clone()));
  }

  /// Writes the project list; a failure is logged, the window keeps working.
  pub(super) fn persist_projects(&self) {
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

  /// Swaps two projects in the project bar while one is dragged over the other.
  pub(crate) fn swap_projects(&mut self, first: &Path, second: &Path, cx: &mut Context<Self>) {
    if self.projects.swap_projects(first, second) {
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

  pub(super) fn confirm_project_removal(&mut self, path: PathBuf, from_settings: bool, window: &mut Window, cx: &mut Context<Self>) {
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
  pub(super) fn remove_project(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
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
  pub(super) fn rename_project_folder(&mut self, path: &Path, name: &str, cx: &mut Context<Self>) -> Result<PathBuf, String> {
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

  pub(super) fn select_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    if self.projects.active.as_ref() != Some(&path) {
      self.switch_project(window, cx, move |projects| projects.active = Some(path));
    }
  }

  /// Changes the open project. Everything the window shows belongs to one project: the folder tree, the Git
  /// summary, the file or diff in the editor and the terminals. What the project being left shows is put away
  /// and comes back when it is opened again; its shells keep running.
  pub(super) fn switch_project(&mut self, window: &mut Window, cx: &mut Context<Self>, change: impl FnOnce(&mut ProjectList)) {
    self.close_project_search(cx);
    self.hovered_folder = None;
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
    self.file_actions.clear_side();
    self.editor_panel.update(cx, |editor, cx| editor.switch_project(before, after, cx));
    self.refresh_project();
    if self.git_view == GitView::History {
      self.load_history(cx);
    }
    self.ensure_terminal(window, cx);
    cx.notify();
  }

  /// Reads the repository summary and the folder tree of the active project.
  pub(super) fn refresh_project(&mut self) {
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
  pub(super) fn sync_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
  pub(super) fn refresh_in_background(&mut self, cx: &mut Context<Self>) {
    self.refresh_with_explorer_reveal(None, cx);
  }

  pub(super) fn refresh_with_explorer_reveal(&mut self, reveal: Option<(PathBuf, ScrollStrategy)>, cx: &mut Context<Self>) {
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
        if let Some((path, strategy)) = &reveal {
          if this.projects.active.as_ref() == Some(&root)
            && this.sidebar_tab == SidebarTab::Explorer
            && this.selected_entry.as_deref() == Some(path.as_path())
          {
            this.scroll_to_explorer_entry(path, *strategy);
            cx.notify();
          }
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
}
