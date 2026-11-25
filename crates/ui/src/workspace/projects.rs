//! The project list, project switching and refreshing.

use super::*;
use std::cell::Cell;

impl Workspace {
  /// Adds the repositories directly inside the projects folder that are not listed yet, and says how many.
  pub(super) fn add_projects_from_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let folder = self.new_project_dir(cx);
    let found = ws_git::find_repositories(&folder);
    let added = self.projects.add_all(found.iter().cloned());
    let message = match (added, found.len()) {
      (_, 0) => format!("No Git repositories in {}.", folder.display()),
      (0, _) => "Every project in that folder is already listed.".to_string(),
      (1, _) => "Added 1 project.".to_string(),
      (count, _) => format!("Added {count} projects."),
    };
    if added > 0 {
      if let Err(error) = self.projects.save(&self.profile.projects_path()) {
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
    self.placement.set(Some(placement));
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

  /// Writes the latest placement now, without the delay, for when the app quits.
  pub(super) fn save_placement_now(&mut self) {
    // A pending delayed write would only repeat this one.
    self.window_save = Task::ready(());
    save_placement(&self.placement);
  }

  /// Opens the dialog that creates, clones or imports a project.
  pub fn add_project(&mut self, _: &AddProject, window: &mut Window, cx: &mut Context<Self>) {
    let location = self.new_project_dir(cx);
    let accounts = self.settings.read(cx).accounts.clone();
    let setup = cx.new(|cx| ProjectSetup::new(location, accounts, window, cx));
    self._setup_subscription = Some(cx.subscribe_in(&setup, window, |this, _, event: &ProjectReady, window, cx| {
      window.close_all_dialogs(cx);
      let path = event.0.clone();
      // The new project gets a tab in the title bar and starts with the default view.
      this.switch_project(window, cx, move |projects| {
        projects.add(path.clone());
        projects.set_favorite(&path, true);
      });
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.p_0().width(px(560.)).child(setup.clone()));
  }

  /// Writes the project list; a failure is logged, the window keeps working.
  pub(super) fn persist_projects(&self) {
    if let Err(error) = self.projects.save(&self.profile.projects_path()) {
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

  /// Stars a branch of the open project, or takes its star away: starred branches are listed first in the branch menu.
  pub(crate) fn toggle_favorite_branch(&mut self, branch: &str, cx: &mut Context<Self>) {
    let Some(path) = self.projects.active.clone() else { return };
    let favorite = self.projects.favorite_branches(&path).iter().any(|name| name == branch);
    self.projects.set_favorite_branch(&path, branch, !favorite);
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

  /// Asks before taking a project out of the app: its shells stop, its folder stays on disk.
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
  pub(super) fn remove_project(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
    if self.project_settings.is_some() {
      self.hide_terminal(window, cx);
    }
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
    crate::recent::forget_project(&self.profile.id, path, cx);
    cx.notify();
  }

  /// Opens a project, or a file inside one, that the Dock menu offered: the project that holds the file comes first.
  pub(crate) fn open_recent(&mut self, path: PathBuf, file: bool, window: &mut Window, cx: &mut Context<Self>) {
    let project = self
      .projects
      .projects
      .iter()
      .filter(|project| if file { path.starts_with(&project.path) } else { path == project.path })
      .max_by_key(|project| project.path.components().count())
      .map(|project| project.path.clone());
    let Some(project) = project else {
      window.push_notification(crate::failure(format!("{} is no longer in this profile.", path.display())), cx);
      crate::recent::forget_project(&self.profile.id, &path, cx);
      return;
    };
    self.select_project(project, window, cx);
    if file {
      self.open_file(path, window, cx);
    }
  }

  /// The activity bar's project settings button: the settings of the open project take the main area, or give it back.
  pub fn toggle_project_settings(&mut self, _: &ToggleProjectSettings, window: &mut Window, cx: &mut Context<Self>) {
    if self.project_settings.is_some() {
      self.hide_terminal(window, cx);
    } else if let Some(path) = self.projects.active.clone() {
      self.open_project_settings(&path, window, cx);
    }
  }

  /// Shows the settings of a project (name, icon, color, Git remote) in the main area.
  pub(crate) fn open_project_settings(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
    let Some(project) = self.projects.projects.iter().find(|project| project.path == path).cloned() else { return };
    // The project list dialog may be what asked.
    window.close_all_dialogs(cx);
    let hosted = self.hosted_repository(&project.path, cx);
    let settings = cx.new(|cx| ProjectSettings::new(&project, hosted, self.renamed_remotes.get(&project.path).cloned(), window, cx));
    self._project_settings_subscription = Some(cx.subscribe_in(&settings, window, |this, _, event: &ProjectSettingsEvent, window, cx| {
      let chosen = match event {
        ProjectSettingsEvent::RemoveRequested(path) => {
          this.confirm_remove_project(path.clone(), window, cx);
          return;
        }
        ProjectSettingsEvent::SyncRenamed(path) => {
          this.sync_renamed_remote(path.clone(), window, cx);
          return;
        }
        ProjectSettingsEvent::Close => {
          this.hide_terminal(window, cx);
          return;
        }
        ProjectSettingsEvent::Save(chosen) => chosen.clone(),
      };
      if chosen.rename_repository {
        this.confirm_repository_rename(chosen, window, cx);
      } else {
        this.apply_project_settings(chosen, None, window, cx);
      }
    }));
    self.terminal_open = false;
    self.agent_open = false;
    self.project_settings = Some(settings);
    self.check_remote_rename(cx);
    cx.notify();
  }

  /// Asks the service, off the interface thread, whether the repository the active project's remote names has another name now.
  /// The answer is kept per project and shown in the project settings and on their button.
  pub(super) fn check_remote_rename(&mut self, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let accounts = self.settings.read(cx).accounts.clone();
    let service_root = root.clone();
    let work = cx.background_executor().spawn(async move {
      let url = ws_git::remote_url(&service_root)?;
      let (connection, repository) = accounts.for_remote(&url, &accounts.keychain())?;
      // A service that no longer knows the old name answers with an error: nothing to tell then.
      let name = ws_hosting::current_name(&connection, &repository).ok()?;
      if name.eq_ignore_ascii_case(&repository.name) {
        return None;
      }
      let new_url = ws_hosting::renamed_url(&url, &repository.name, &name)?;
      Some(RenamedRemote {
        service: connection.account.provider.label().into(),
        old_full_name: repository.full_name(),
        old_name: repository.name,
        new_name: name,
        new_url,
      })
    });
    cx.spawn(async move |this, cx| {
      let found = work.await;
      let _ = this.update(cx, |this, cx| {
        match &found {
          Some(renamed) => this.renamed_remotes.insert(root.clone(), renamed.clone()),
          None => this.renamed_remotes.remove(&root),
        };
        if let Some(settings) = &this.project_settings {
          settings.update(cx, |settings, cx| settings.set_renamed(found, cx));
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Points the remote at the repository's new address and, when the project was named like the old repository, names the project
  /// like the new one. The folder on disk is left alone.
  fn sync_renamed_remote(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let Some(renamed) = self.renamed_remotes.get(&path).cloned() else { return };
    if let Err(message) = ws_git::set_remote_url(&path, &renamed.new_url) {
      window.push_notification(crate::failure(format!("Could not change the Git remote: {message}")), cx);
      return;
    }
    if let Some(project) = self.projects.projects.iter().find(|project| project.path == path).cloned() {
      if project.name().eq_ignore_ascii_case(&renamed.old_name) {
        self.projects.set_appearance(&path, &renamed.new_name, project.icon.as_deref(), project.color.as_deref());
        self.persist_projects();
      }
    }
    self.renamed_remotes.remove(&path);
    if self.projects.active.as_ref() == Some(&path) {
      self.refresh_project();
    }
    window.push_notification(Notification::info(format!("Synced with the new repository name {}.", renamed.new_name)), cx);
    self.hide_terminal(window, cx);
  }

  /// The repository a project's remote names on a service one of the connected accounts serves.
  fn hosted_repository(&self, path: &Path, cx: &App) -> Option<HostedRepository> {
    let url = ws_git::remote_url(path)?;
    let accounts = &self.settings.read(cx).accounts;
    accounts.items.iter().find_map(|account| {
      let repository = RepoRef::for_account(account, &url)?;
      Some(HostedRepository { service: account.provider.label().into(), full_name: repository.full_name(), name: repository.name })
    })
  }

  /// Asks before the repository is renamed on its service: it changes something for everyone who uses it, and it cannot be taken back
  /// from here.
  fn confirm_repository_rename(&mut self, chosen: AppearanceChosen, window: &mut Window, cx: &mut Context<Self>) {
    let Some(hosted) = self.hosted_repository(&chosen.path, cx) else {
      // The account was disconnected since the page opened: the rest of the settings are still saved.
      let chosen = AppearanceChosen { rename_repository: false, ..chosen };
      self.apply_project_settings(chosen, None, window, cx);
      return;
    };
    let workspace = cx.entity();
    let (title, description) = (
      SharedString::from(format!("Rename the repository on {}?", hosted.service)),
      SharedString::from(format!(
        "{} will be renamed to {} on {}. Anyone who cloned it has to update their remote address; this project's remote is updated for you.",
        hosted.full_name,
        chosen.name.trim(),
        hosted.service
      )),
    );
    window.open_alert_dialog(cx, move |alert, _, _| {
      let (workspace, chosen) = (workspace.clone(), chosen.clone());
      alert.title(title.clone()).description(description.clone()).width(px(460.)).confirm().ok_text("Rename").on_ok(move |_, window, cx| {
        workspace.update(cx, |this, cx| this.rename_repository_then_save(chosen.clone(), window, cx));
        true
      })
    });
  }

  /// Renames the repository on its service in the background; only when that worked are the settings applied, with the remote pointing
  /// at the new address. A refusal changes nothing.
  fn rename_repository_then_save(&mut self, chosen: AppearanceChosen, window: &mut Window, cx: &mut Context<Self>) {
    let accounts = self.settings.read(cx).accounts.clone();
    let path = chosen.path.clone();
    let new_name = chosen.name.trim().to_string();
    let work = cx.background_executor().spawn(async move {
      let url = ws_git::remote_url(&path).ok_or("The project has no Git remote.")?;
      let (connection, repository) = accounts
        .for_remote(&url, &accounts.keychain())
        .ok_or("The token for this account is not in the Keychain. Connect the account again in Settings → Accounts.")?;
      let name = ws_hosting::rename_repository(&connection, &repository, &new_name)?;
      let renamed = ws_hosting::renamed_url(&url, &repository.name, &name).ok_or("The remote address does not name the repository.")?;
      Ok::<_, String>(renamed)
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| match result {
        Ok(url) => this.apply_project_settings(chosen, Some(url), window, cx),
        Err(message) => window.push_notification(crate::failure(format!("Could not rename the repository: {message}")), cx),
      });
    })
    .detach();
  }

  /// Saves what the settings view chose. `remote` is the new remote address when the repository was just renamed on its service;
  /// otherwise the address typed in the view, if it changed, is used. The remote is changed first: if that fails, nothing else is.
  fn apply_project_settings(&mut self, chosen: AppearanceChosen, remote: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
    let remote = remote.or_else(|| chosen.remote.clone());
    if let Some(url) = &remote {
      if let Err(message) = ws_git::set_remote_url(&chosen.path, url) {
        window.push_notification(crate::failure(format!("Could not change the Git remote: {message}")), cx);
        return;
      }
    }
    let mut path = chosen.path.clone();
    if chosen.rename_folder {
      match self.rename_project_folder(&chosen.path, chosen.name.trim(), cx) {
        Ok(moved) => path = moved,
        Err(message) => {
          window.push_notification(crate::failure(format!("Could not rename the folder: {message}")), cx);
          return;
        }
      }
    }
    self.projects.set_appearance(&path, &chosen.name, chosen.icon.as_deref(), chosen.color.as_deref());
    self.persist_projects();
    if chosen.rename_folder {
      self.terminal_tabs.remove(&chosen.path);
      self.agent_tabs.remove(&chosen.path);
      self.views.remove(&chosen.path);
    }
    if self.projects.active.as_ref() == Some(&path) {
      self.refresh_project();
      if chosen.rename_folder {
        self.ensure_terminal(window, cx);
      }
    }
    window.push_notification(
      Notification::info(if chosen.rename_repository { "Project settings saved and the repository renamed." } else { "Project settings saved." }),
      cx,
    );
    self.hide_terminal(window, cx);
  }

  /// Takes the project settings view off the main area (the caller decides what is shown instead and where the keyboard goes).
  pub(super) fn leave_project_settings(&mut self) {
    self.project_settings = None;
    self._project_settings_subscription = None;
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
    self.hovered_folder = None;
    self.leave_project_settings();
    let before = self.projects.active.clone();
    if let Some(root) = &before {
      let view = ProjectView {
        sidebar_tab: self.sidebar_tab,
        git_view: self.git_view,
        terminal_open: self.terminal_open,
        agent_open: self.agent_open,
        expanded: std::mem::take(&mut self.expanded),
        selected_entry: self.selected_entry.take(),
        selected_change: self.selected_change.take(),
      };
      self.explorer_selection.clear();
      self.selection_anchor = None;
      self.views.insert(root.clone(), view);
      // What the tab of the project being left shows comes from here on from what is remembered, which can be ten seconds old: remember
      // what the window knows now, so a push that just ended is not shown as waiting.
      let status = self.git.as_ref().map(|git| {
        let (changed, push) = git.status();
        ProjectStatus { changed, push }
      });
      job_scope::remember_status(&mut self.project_status, root, status);
      // The commit message typed for it stays with it.
      let draft =
        Draft { summary: self.commit_summary.read(cx).value().to_string(), description: self.commit_description.read(cx).value().to_string() };
      self.drafts.put_away(root, draft);
    }
    self.close_project_search(cx);
    change(&mut self.projects);
    if let Err(error) = self.projects.save(&self.profile.projects_path()) {
      eprintln!("Could not save the project list: {error}");
    }
    if let Some(active) = self.projects.active.clone() {
      crate::recent::record_project(&self.profile.id, &active, cx);
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
    self.sidebar_tab = view.sidebar_tab;
    self.git_view = view.git_view;
    self.terminal_open = view.terminal_open;
    self.agent_open = view.agent_open;
    self.expanded = view.expanded;
    self.selected_entry = view.selected_entry;
    self.selected_change = view.selected_change;
    let draft = after.as_ref().map(|root| self.drafts.take(root)).unwrap_or_default();
    self.commit_summary.update(cx, |input, cx| input.set_value(draft.summary, window, cx));
    self.commit_description.update(cx, |input, cx| input.set_value(draft.description, window, cx));
    if let (Some(root), Some(file)) = (after.clone(), self.selected_change.clone()) {
      self.change_view.update(cx, |view, cx| view.show(root, file, None, cx));
    }
    self.file_actions.clear_side();
    self.editor_panel.update(cx, |editor, cx| editor.switch_project(before, after, cx));
    self.refresh_project();
    if self.git_view == GitView::History {
      self.load_history(cx);
    }
    self.ensure_terminal(window, cx);
    self.warm_index(cx);
    self.check_remote_rename(cx);
    if self.sidebar_tab == SidebarTab::Search {
      self.ensure_project_search(window, cx);
    }
    if self.terminal_open {
      self.focus_terminal(window, cx);
    } else if self.agent_open {
      self.focus_agent(window, cx);
    } else if self.sidebar_tab == SidebarTab::Search {
      self.search_state.read(cx).focus_handle(cx).focus(window, cx);
    }
    cx.notify();
  }

  /// Makes the index of the open project ready in the background: the saved one is checked and used, or the tree is walked once and
  /// saved. Searching the project then starts from a ready list of files.
  pub(crate) fn warm_index(&mut self, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else {
      self.index_task = Task::ready(());
      return;
    };
    let work = cx.background_executor().spawn(async move {
      ws_index::ProjectIndex::open(&ws_storage::index_dir(), &root);
    });
    self.index_task = cx.spawn(async move |_, _| work.await);
  }

  /// Reads the repository summary and the folder tree of the active project.
  pub(super) fn refresh_project(&mut self) {
    match &self.projects.active {
      Some(root) => {
        self.git = ws_git::summary(root);
        self.rows = explorer::rows(root, &self.expanded);
        self.remember_remote();
      }
      None => {
        self.git = None;
        self.rows.clear();
      }
    }
  }

  /// Finds the projects whose folder was deleted or moved, off the interface thread. They stay in the list: opening one shows what can be
  /// done (locate the folder, clone it again, remove the project). A project whose folder came back is shown again.
  pub(super) fn sync_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.syncing || self.projects.projects.is_empty() {
      return;
    }
    self.syncing = true;
    let list = self.projects.clone();
    let work = cx.background_executor().spawn(async move { list.missing_paths() });
    cx.spawn_in(window, async move |this, cx| {
      let missing: HashSet<PathBuf> = work.await.into_iter().collect();
      let _ = this.update_in(cx, |this, window, cx| {
        this.syncing = false;
        if this.missing_projects == missing {
          return;
        }
        let returned = this.projects.active.as_ref().is_some_and(|root| this.missing_projects.contains(root) && !missing.contains(root));
        this.missing_projects = missing;
        if returned {
          this.refresh_project();
          this.ensure_terminal(window, cx);
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Reads the repository and the folder tree off the interface thread and shows them only if they changed.
  pub(super) fn refresh_in_background(&mut self, cx: &mut Context<Self>) {
    self.refresh_with_explorer_reveal(None, cx);
  }

  /// Colors the diffs of the files that just got changes (or whose change differs), in the background, so opening one finds its colors
  /// ready. Files whose change is the same as before are not touched: their hunks are still in the cache.
  fn warm_new_changes(&self, root: &Path, git: Option<&ws_git::Summary>, cx: &mut Context<Self>) {
    const MOST_FILES: usize = 12;
    let Some(git) = git else { return };
    let known = self.git.as_ref().map(|git| git.changes.as_slice()).unwrap_or_default();
    let files: Vec<PathBuf> =
      git.changes.iter().filter(|change| !known.contains(change)).map(|change| change.path.clone()).take(MOST_FILES).collect();
    if files.is_empty() {
      return;
    }
    let (root, theme) = (root.to_path_buf(), crate::diff_view::highlight_theme(cx));
    cx.background_executor()
      .spawn(async move {
        for file in files {
          if let Ok(patch) = ws_git::file_diff(&root, &file) {
            crate::diff_view::warm(&theme, &patch);
          }
        }
      })
      .detach();
  }

  pub(super) fn refresh_with_explorer_reveal(&mut self, reveal: Option<(PathBuf, ScrollStrategy)>, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let expanded = self.expanded.clone();
    let work = cx.background_executor().spawn({
      let (root, expanded) = (root.clone(), expanded.clone());
      async move {
        // Every two seconds: only a slow or failed round is worth a line in the log.
        ws_activity::timed_quiet(
          "ui",
          REFRESH_NOTABLE_MS,
          || format!("refresh {} (git status + folder tree)", root.display()),
          || (ws_git::summary(&root), explorer::rows(&root, &expanded), ws_git::history_stamp(&root)),
          |(git, rows, _)| match git {
            Some(_) => format!("ok, {} rows", rows.len()),
            None => format!("failed: not a Git repository, {} rows", rows.len()),
          },
        )
      }
    });
    cx.spawn(async move |this, cx| {
      let (git, rows, stamp) = work.await;
      let _ = this.update(cx, |this, cx| {
        // The user may have switched projects or opened a folder while this was reading.
        if this.projects.active.as_ref() == Some(&root) && this.expanded == expanded && (this.git != git || this.rows != rows) {
          this.warm_new_changes(&root, git.as_ref(), cx);
          this.git = git;
          this.rows = rows;
          this.remember_remote();
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
        // The commits are read again only when HEAD or its upstream moved, not every round.
        if this.projects.active.as_ref() == Some(&root) && this.history_stamp != stamp {
          this.history_stamp = stamp;
          if this.sidebar_tab == SidebarTab::Git && this.git_view == GitView::History {
            this.refresh_history(cx);
          }
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

/// Writes the placement in `latest` (once) to `window.json`; a failure is logged.
pub(super) fn save_placement(latest: &Cell<Option<WindowPlacement>>) {
  if let Some(placement) = latest.take() {
    if let Err(error) = placement.save(&WindowPlacement::default_path()) {
      eprintln!("Could not save the window size: {error}");
    }
  }
}
