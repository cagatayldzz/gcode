//! Menus, settings pages and Git hosting accounts.

use super::*;

impl Workspace {
  /// The page of the open project's repository on the service that hosts it, and the service's name.
  pub(super) fn web_repository(&self, cx: &App) -> Option<ws_hosting::WebRepository> {
    let url = self.git.as_ref()?.sync.as_ref()?.url.clone();
    ws_hosting::web_repository(&url, &self.settings.read(cx).accounts.items)
  }

  /// Puts the service of the open project's remote in the Repository menu ("View on GitHub"). Menus belong to the application,
  /// so only the window in front sets them: always when it comes to the front, else only when the service changed.
  pub(super) fn sync_menus(&mut self, always: bool, cx: &mut Context<Self>) {
    if !self.window_active {
      return;
    }
    let host = self.web_repository(cx).map(|found| found.service);
    let profiles = crate::profiles::profile_list(cx).profiles;
    if always || host != self.menu_host || profiles != self.menu_profiles {
      cx.set_menus(crate::menus(host.as_deref(), &profiles, &self.profile.id));
      self.menu_host = host;
      self.menu_profiles = profiles;
    }
  }

  /// Repository → View on GitHub, GitLab or Azure DevOps: opens the repository's page in the browser.
  pub fn view_on_host(&mut self, _: &ViewOnHost, window: &mut Window, cx: &mut Context<Self>) {
    match self.web_repository(cx) {
      Some(found) => cx.open_url(&found.url),
      None => window.push_notification(crate::failure("This project has no remote on a hosting service."), cx),
    }
  }

  /// Opens the Settings, or closes them when they are open.
  pub fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
    if self.app_settings.is_some() {
      self.hide_terminal(window, cx);
      return;
    }
    self.open_settings_page("General", window, cx);
  }

  /// Opens Settings on the Updates page and looks for a newer release.
  pub fn check_for_updates(&mut self, _: &CheckForUpdates, window: &mut Window, cx: &mut Context<Self>) {
    self.open_settings_page("About", window, cx);
    self.settings.update(cx, |model, cx| model.check_update(cx));
  }

  /// Marks the account that serves `url` as having a permission problem, because the host refused its token: a warning shows by
  /// Settings until a check finds it fixed. `pull_requests` says which permission was refused.
  pub(super) fn flag_account_issue(&mut self, url: &str, kind: IssueKind, cx: &mut Context<Self>) {
    self.settings.update(cx, |model, cx| {
      let Some(account) = model.accounts.items.iter_mut().find(|account| RepoRef::for_account(account, url).is_some()) else { return };
      let (action, permission) = match kind {
        IssueKind::PullRequests => ("Opening a pull request", account.provider.pull_request_permission().to_string()),
        IssueKind::Push => ("Pushing or pulling", account.provider.push_permission().to_string()),
        IssueKind::Workflows => {
          ("Pushing commits that change .github/workflows", format!("{} (a classic token, the `workflow` scope)", ws_git::WORKFLOWS_REFUSED))
        }
      };
      account.issue = Some(format!("{action} was refused with this token. It needs {permission}."));
      if let Err(error) = model.accounts.save(&model.profile.accounts_path()) {
        eprintln!("Could not save the accounts: {error}");
      }
      cx.notify();
    });
  }

  /// Shows a failed push, pull or fetch. When connecting the account again would fix it, the notification has a **Reconnect
  /// account** button that opens the connect dialog for the account that serves `url` (or an empty one when none does).
  pub(super) fn push_remote_failure(&mut self, message: String, url: &str, window: &mut Window, cx: &mut Context<Self>) {
    let mut notification = crate::failure(message.clone());
    if needs_reconnect(&message) {
      let account = self.settings.read(cx).accounts.items.iter().find(|account| RepoRef::for_account(account, url).is_some()).cloned();
      let workspace = cx.entity().downgrade();
      // As content, the button sits under the message (an action would stand beside it and squeeze the text); it keeps the
      // notification open, as an action does.
      notification = notification.autohide(false).content(move |_, _, cx| {
        let (workspace, account) = (workspace.clone(), account.clone());
        h_flex()
          .pt_2()
          .child(Button::new("reconnect-account").label("Reconnect account").small().primary().on_click(cx.listener(move |this, _, window, cx| {
            this.dismiss(window, cx);
            let account = account.clone();
            let _ = workspace.update(cx, |workspace, cx| workspace.open_connect_account(account, window, cx));
          })))
          .into_any_element()
      });
    }
    window.push_notification(notification, cx);
  }

  /// Sets or clears what is wrong with an account's token.
  pub(super) fn set_account_issue(&mut self, key: &str, issue: Option<String>, cx: &mut Context<Self>) {
    self.settings.update(cx, |model, cx| {
      if let Some(account) = model.accounts.items.iter_mut().find(|account| account.key() == key) {
        account.issue = issue;
      }
      if let Err(error) = model.accounts.save(&model.profile.accounts_path()) {
        eprintln!("Could not save the accounts: {error}");
      }
      cx.notify();
    });
  }

  /// Asks the service what the account's token may do (see `ws_hosting::verify_permissions`; nothing is changed) in the open
  /// project's repository if the account serves it, else in the account's first repository. A fixed account loses its warning.
  pub(crate) fn recheck_account(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
    let Some(account) = self.settings.read(cx).accounts.items.iter().find(|account| account.key() == key).cloned() else { return };
    if self.settings.read(cx).rechecking.contains(&key) {
      return;
    }
    self.settings.update(cx, |model, cx| {
      model.rechecking.push(key.clone());
      cx.notify();
    });
    let remote = self.projects.active.as_ref().and_then(|root| ws_git::remote_url(root));
    let keychain = self.settings.read(cx).accounts.keychain();
    let work = cx.background_executor().spawn({
      let account = account.clone();
      async move {
        let token = keychain
          .get(&account.key())
          .ok_or("The token is not in the Keychain any more. Disconnect the account and connect it again.".to_string())?;
        let connection = Connection { account: account.clone(), token };
        // The user asked to check again: what was read before is asked again.
        connection.expire();
        let own = remote.and_then(|url| RepoRef::for_account(&account, &url).map(|repository| (repository, url)));
        let (repository, url) = match own {
          Some(found) => found,
          None => {
            let first =
              ws_hosting::repositories(&connection)?.into_iter().next().ok_or("This account has no repository to check the permissions in.")?;
            let repository = RepoRef::for_account(&account, &first.clone_url).ok_or("Could not tell which repository that is.")?;
            (repository, first.clone_url)
          }
        };
        let report = ws_hosting::verify_permissions(&connection, &repository, &url);
        let confirmed = report.push.is_some() || report.pull_requests.is_some();
        let problems = if confirmed { report.missing(account.provider) } else { ws_hosting::check_access(&connection).warnings(account.provider) };
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
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
      });
    })
    .detach();
  }

  /// Opens Settings on one of its pages ("General", "Accounts", …) in the main area; if they are open already, the page changes.
  pub(crate) fn open_settings_page(&mut self, page: &'static str, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(settings) = &self.app_settings {
      settings.update(cx, |settings, cx| settings.show_page(page, cx));
      return;
    }
    let fallback = self.new_project_dir(cx);
    let workspace = cx.entity();
    let projects = cx.new({
      let workspace = workspace.clone();
      |cx| ProjectManager::new(workspace, cx)
    });
    let profiles = cx.new({
      let workspace = workspace.clone();
      |cx| ProfileManager::new(workspace, cx)
    });
    let accounts = cx.new({
      let (settings, workspace) = (self.settings.clone(), workspace.clone());
      |cx| AccountsList::new(settings, workspace, cx)
    });
    let updates = cx.new({
      let settings = self.settings.clone();
      |cx| UpdatePanel::new(settings, cx)
    });
    let settings = self.settings.clone();
    let view = cx.new(|cx| SettingsWindow::new(settings, workspace, fallback, projects, profiles, accounts, updates, page, window, cx));
    self.leave_project_settings();
    self.terminal_open = false;
    self.agent_open = false;
    self.app_settings = Some(view);
    cx.notify();
  }
}
