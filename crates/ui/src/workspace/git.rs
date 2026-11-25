//! Git state: remotes, history, comparing, branches and project status.

use super::*;

impl Workspace {
  /// Does what the toolbar's button says: pulls when the remote is ahead, pushes (or publishes) when the branch is, else fetches.
  /// The button shows what git reports while it works.
  pub(super) fn sync_remote(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (Some(root), Some(sync)) = (self.projects.active.clone(), self.git.as_ref().and_then(|git| git.sync.clone())) else { return };
    if self.remote_work.is_some() || self.merge_task.is_some() || self.git.as_ref().is_some_and(|git| git.merging) {
      return;
    }
    let remote = sync.remote.clone();
    let remote_url = sync.url.clone();
    let title = if sync.behind > 0 {
      format!("Pulling from {remote}")
    } else if !sync.upstream {
      "Publishing branch".to_string()
    } else if sync.ahead > 0 {
      format!("Pushing to {remote}")
    } else {
      format!("Fetching {remote}")
    };
    let progress = Arc::new(Mutex::new(String::from("Connecting…")));
    let fetching_after_push = Arc::new(AtomicBool::new(false));
    let fraction = Arc::new(AtomicU32::new(0));
    self.remote_work = Some(RemoteWork {
      root: root.clone(),
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
          ws_git::pull(&root, credentials.as_ref(), |line| report(0., 1., line))
        } else if sync.ahead > 0 || !sync.upstream {
          let pushed = ws_git::push(&root, credentials.as_ref(), |line| report(0., 0.85, line))?;
          report(0.85, 0.15, "Connecting…");
          fetching_after_push.store(true, Ordering::Relaxed);
          ws_git::fetch(&root, credentials.as_ref(), |line| report(0.85, 0.15, line))
            .map_err(|error| format!("Push succeeded, but fetching failed: {error}"))?;
          Ok(pushed)
        } else {
          ws_git::fetch(&root, credentials.as_ref(), |line| report(0., 1., line))
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
          this.remote_finish = Some((work.shown(), run, work.root.clone()));
          cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REMOTE_FINISH * 3 / 2).await;
            let _ = this.update(cx, |this, cx| {
              if this.remote_finish.as_ref().is_some_and(|(_, current, _)| *current == run) {
                this.remote_finish = None;
                cx.notify();
              }
            });
          })
          .detach();
        }
        // The window may show another project by now: settle the one the work was for, and say which it was.
        this.settle_job(&root, cx);
        match result {
          Ok(_) => {}
          Err(message) => {
            if let Some(kind) = refusal_kind(&message) {
              this.flag_account_issue(&remote_url, kind, cx);
            }
            this.push_remote_failure(message, &remote_url, window, cx)
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
  pub(super) fn hosted_remote(&self, cx: &App) -> Option<(String, RepoRef)> {
    let url = self.git.as_ref()?.sync.as_ref()?.url.clone();
    let accounts = &self.settings.read(cx).accounts;
    accounts.items.iter().find_map(|account| RepoRef::for_account(account, &url)).map(|repository| (url, repository))
  }

  /// Switches the Git tab between the files to commit and the commits already made.
  pub(super) fn set_git_view(&mut self, view: GitView, cx: &mut Context<Self>) {
    if self.git_view == view {
      return;
    }
    self.git_view = view;
    // Going back and forth between Changes and History reads the commits only when they changed since the list was read.
    if view == GitView::History && !self.history_is_current() {
      self.load_history(cx);
    }
    cx.notify();
  }

  /// Whether the list shown is what the repository has now: it was read for this project and page size, and HEAD and its upstream have
  /// not moved since (`history_stamp` is renewed by every refresh).
  fn history_is_current(&self) -> bool {
    match (&self.history_loaded, &self.projects.active, &self.history_stamp) {
      (Some((root, limit, stamp)), Some(active), Some(now)) => root == active && *limit == self.history_limit && stamp == now,
      _ => false,
    }
  }

  /// Reads the commits of the open project off the interface thread; the first one is shown when none is chosen.
  /// The list shows a loader until the commits arrive, so a switch of branch or project is noticed.
  pub(super) fn load_history(&mut self, cx: &mut Context<Self>) {
    self.read_history(true, cx);
  }

  /// Reads the commits again without replacing the list by the loader, for the periodic refresh: the list stays as it is until
  /// the new commits differ.
  pub(super) fn refresh_history(&mut self, cx: &mut Context<Self>) {
    self.read_history(false, cx);
  }

  /// `show_loader` is off when a page is added to a list that stays on screen.
  fn read_history(&mut self, show_loader: bool, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    self.history_epoch += 1;
    if show_loader && !self.history_loading {
      self.history_loading = true;
      cx.notify();
    }
    let epoch = self.history_epoch;
    let limit = self.history_limit;
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move {
        // The periodic read (no loader) is logged only when it is slow or fails.
        ws_activity::timed_quiet(
          "ui",
          if show_loader { 0 } else { REFRESH_NOTABLE_MS },
          || format!("read history {} (limit {limit})", root.display()),
          || (ws_git::history_stamp(&root), ws_git::history(&root, limit)),
          |(_, result)| match result {
            Ok(commits) => format!("ok, {} commits", commits.len()),
            Err(error) => format!("failed: {error}"),
          },
        )
      }
    });
    cx.spawn(async move |this, cx| {
      let (stamp, found) = work.await;
      let read = found.is_ok();
      let found = found.unwrap_or_default();
      let _ = this.update(cx, |this, cx| {
        if this.history_epoch != epoch || this.projects.active.as_ref() != Some(&root) {
          return;
        }
        this.history_loaded = if read { stamp.map(|stamp| (root.clone(), limit, stamp)) } else { None };
        if this.history_loading {
          this.history_loading = false;
          cx.notify();
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
  pub(super) fn undo_commit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move { ws_git::undo_commit(&root, &id) }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        match result {
          Ok((summary, description)) => {
            if this.settle_job(&root, cx) {
              this.commit_summary.update(cx, |input, cx| input.set_value(summary, window, cx));
              this.commit_description.update(cx, |input, cx| input.set_value(description, window, cx));
              this.selected_commit = None;
              this.git_view = GitView::Changes;
            } else {
              // The message waits in that project's box until it is opened again.
              this.drafts.set(&root, Draft { summary, description });
            }
            let message = this.say_where(&root, "Undid the commit: its changes are back in Changes, with its message in the commit box.".to_string());
            window.push_notification(Notification::info(message), cx);
          }
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Asks for a name and tags a commit with it.
  pub(super) fn create_tag(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
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
        ws_git::create_tag(&root, &id, name)?;
        this.load_history(cx);
        window.push_notification(Notification::info(format!("Tagged {short} as {}. The tag is local until you push it.", name.trim())), cx);
        Ok(())
      },
    );
  }

  /// Loads the next page of commits when the History list has been scrolled to its end. Called after the list was scrolled, once its
  /// position has settled, and by the button at the end of the list.
  pub(super) fn load_more_history(&mut self, only_at_end: bool, cx: &mut Context<Self>) {
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
    self.read_history(false, cx);
  }

  /// Opens the list of branches under the compare field, reading them off the interface thread.
  pub(super) fn open_compare_list(&mut self, _: &mut Window, cx: &mut Context<Self>) {
    if self.compare_open || self.compare.is_some() {
      return;
    }
    let Some(root) = self.projects.active.clone() else { return };
    self.compare_open = true;
    self.compare_choices = None;
    self.compare_epoch += 1;
    let epoch = self.compare_epoch;
    let work = cx.background_executor().spawn(async move { ws_git::compare_choices(&root) });
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
  pub(super) fn close_compare_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.compare_open = false;
    self.compare_epoch += 1;
    self.compare_filter.update(cx, |input, cx| input.set_value("", window, cx));
    window.blur(cx);
    cx.notify();
  }

  /// The branches the field's text matches, in the list's three groups.
  pub(super) fn matching_compare_branches(&self, cx: &App) -> (Option<BranchRef>, Vec<BranchRef>, Vec<BranchRef>) {
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
  pub(super) fn choose_first_compare_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (default, recent, other) = self.matching_compare_branches(cx);
    if let Some(branch) = default.into_iter().chain(recent).chain(other).next() {
      self.choose_compare_branch(branch.name, window, cx);
    }
  }

  /// Compares the History list with a branch: the commits each side has that the other lacks, read off the interface thread.
  pub(super) fn choose_compare_branch(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    self.close_compare_list(window, cx);
    self.compare = Some(CompareView { branch: name.clone(), side: CompareSide::Behind, found: None });
    let epoch = self.compare_epoch;
    let work = cx.background_executor().spawn({
      let name = name.clone();
      async move { ws_git::compare(&root, &name) }
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
            window.push_notification(crate::failure(message), cx);
          }
        }
        cx.notify();
      });
    })
    .detach();
    cx.notify();
  }

  /// Shows the other side of the comparison.
  pub(super) fn set_compare_side(&mut self, side: CompareSide, cx: &mut Context<Self>) {
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
  pub(super) fn clear_compare(&mut self, cx: &mut Context<Self>) {
    self.compare = None;
    self.compare_epoch += 1;
    if let Some(id) = self.history.first().map(|commit| commit.id.clone()) {
      self.select_commit(id, cx);
    }
    cx.notify();
  }

  /// Starts the explicitly requested merge or reopens its conflict resolver.
  pub(super) fn merge_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
          ws_git::read_merge(&root).map(ws_git::MergeOutcome::Conflicted)
        } else {
          ws_git::start_merge(&root, branch.as_deref().unwrap_or_default(), &current)
        }
      }
    });
    self.merge_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.merge_task = None;
        if !this.settle_job(&root, cx) {
          cx.notify();
          return;
        }
        match result {
          Ok(ws_git::MergeOutcome::Finished) => this.merge_completed(branch.as_deref(), window, cx),
          Ok(ws_git::MergeOutcome::Conflicted(state)) => {
            if let Some((_, dialog, _)) = this.merge_dialog.as_ref().filter(|(path, dialog, _)| path == &root && dialog.read(cx).same_merge(&state)) {
              dialog.update(cx, |dialog, cx| dialog.set_state(state, cx));
              Self::show_merge_dialog(dialog.clone(), window, cx);
              cx.notify();
              return;
            }
            let source = if pending { "incoming changes".into() } else { branch.unwrap_or_default().into() };
            let dialog = cx.new(|_| MergeDialog::new(root.clone(), source, state));
            let subscription = cx.subscribe_in(&dialog, window, |this, dialog, event, window, cx| {
              if let MergeEvent::OpenFile(path) = event {
                window.close_dialog(cx);
                this.open_changed_file(&OpenChangedFile(path.clone()), window, cx);
                return;
              }
              let source = dialog.read(cx).source().clone();
              window.close_dialog(cx);
              this.merge_dialog = None;
              this.refresh_project();
              match event {
                MergeEvent::Completed => this.merge_completed(Some(source.as_str()), window, cx),
                MergeEvent::Aborted => window.push_notification(Notification::info("Merge aborted."), cx),
                MergeEvent::OpenFile(_) => {}
              }
              cx.notify();
            });
            this.merge_dialog = Some((root, dialog.clone(), subscription));
            Self::show_merge_dialog(dialog, window, cx);
          }
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
        cx.notify();
      });
    }));
    cx.notify();
  }

  pub(super) fn show_merge_dialog(dialog: Entity<MergeDialog>, window: &mut Window, cx: &mut App) {
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

  pub(super) fn merge_completed(&mut self, source: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
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
  pub(super) fn select_commit(&mut self, id: String, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    self.selected_commit = Some(id.clone());
    self.commit_view.update(cx, |view, cx| view.show(root, id, cx));
    cx.notify();
  }

  /// Shows a tab of the branch menu, with an empty filter.
  pub(super) fn set_branch_tab(&mut self, tab: BranchTab, window: &mut Window, cx: &mut Context<Self>) {
    if self.branch_tab == tab {
      return;
    }
    self.branch_tab = tab;
    self.branch_filter.update(cx, |input, cx| input.set_value("", window, cx));
    if tab == BranchTab::PullRequests {
      self.load_pull_requests(false, cx);
    }
    cx.notify();
  }

  /// Reads the open pull requests of the repository off the interface thread. What was read a minute ago is used again; `fresh` (the
  /// Refresh button) asks the service, which costs nothing against its limit when nothing changed.
  pub(super) fn load_pull_requests(&mut self, fresh: bool, cx: &mut Context<Self>) {
    let (Some((url, _)), Some(root)) = (self.hosted_remote(cx), self.projects.active.clone()) else { return };
    let accounts = self.settings.read(cx).accounts.clone();
    self.pull_requests = PullRequests::Loading;
    self.pr_epoch += 1;
    let epoch = self.pr_epoch;
    let work = cx.background_executor().spawn(async move {
      let (connection, repository) = accounts
        .for_remote(&url, &accounts.keychain())
        .ok_or("The token for this account is not in the Keychain. Connect the account again in Settings → Accounts.")?;
      if fresh {
        connection.expire();
      }
      ws_hosting::pull_requests(&connection, &repository)
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
  pub(super) fn open_pull_request_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.branch_menu_open = false;
    cx.notify();
    let (Some(root), Some(git), Some((url, _))) = (self.projects.active.clone(), self.git.as_ref(), self.hosted_remote(cx)) else { return };
    let Some(sync) = git.sync.as_ref() else { return };
    let target = PullRequestTarget {
      remote_url: url,
      source: git.branch.clone(),
      unpushed: sync.ahead,
      published: sync.upstream,
      suggested_title: ws_git::head_summary(&root).unwrap_or_else(|| git.branch.clone()),
    };
    let accounts = self.settings.read(cx).accounts.clone();
    let form = cx.new(|cx| PullRequestForm::new(accounts, target, window, cx));
    self._dialog_subscription = Some(cx.subscribe_in(&form, window, |this, form, event: &PullRequestEvent, window, cx| match event {
      PullRequestEvent::Opened(opened) => {
        window.close_dialog(cx);
        window.push_notification(Notification::info(format!("Opened pull request #{}: {}", opened.number, opened.title)), cx);
        cx.open_url(&opened.url);
        this.load_pull_requests(false, cx);
      }
      PullRequestEvent::Refused => {
        let url = form.read(cx).remote_url().to_string();
        this.flag_account_issue(&url, IssueKind::PullRequests, cx);
      }
    }));
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Create a pull request").width(px(600.)).p_5().child(form.clone()));
  }

  /// Opens or closes the branch menu; opening reads the local branches and clears the filter.
  pub(super) fn set_branch_menu(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
    self.branch_menu_open = open;
    if open {
      self.branch_tab = BranchTab::Branches;
      if let Some(root) = &self.projects.active {
        self.branches = ws_git::branch_entries(root).unwrap_or_default();
      }
      self.branch_filter.update(cx, |input, cx| input.set_value("", window, cx));
    }
    cx.notify();
  }

  /// Asks for a name, creates a branch at the current commit and switches to it. Uncommitted changes come along.
  pub(super) fn new_branch(&mut self, initial: String, window: &mut Window, cx: &mut Context<Self>) {
    self.branch_menu_open = false;
    let Some(current) = self.git.as_ref().map(|git| git.branch.clone()) else { return };
    let default = self.branches.iter().find(|entry| entry.default && entry.remote.is_none()).map(|entry| entry.name.clone());
    let about_default = match default {
      Some(default) if default == current => format!(" {default} is the default branch for your repository."),
      Some(default) => format!(" The default branch of your repository is {default}."),
      None => String::new(),
    };
    let text = SharedString::from(format!("Your new branch will be based on your currently checked out branch ({current}).{about_default}"));
    let input = cx.new(|cx| InputState::new(window, cx).default_value(initial));
    self._dialog_subscription = Some(cx.subscribe_in(&input, window, |this, input, event: &InputEvent, window, cx| {
      if let InputEvent::PressEnter { .. } = event {
        let name = input.read(cx).value().to_string();
        this.create_branch_named(&name, window, cx);
      }
    }));
    let workspace = cx.entity();
    let field = input.clone();
    let muted = cx.theme().muted_foreground;
    window.open_dialog(cx, move |dialog, _, cx| {
      let (create, name) = (workspace.clone(), field.clone());
      dialog
        .title("Create a Branch")
        .width(px(480.))
        .p_5()
        .child(
          v_flex()
            .gap_2()
            .child(Label::new("Name").font_weight(FontWeight::SEMIBOLD))
            .child(crate::components::soft_input(&field, cx))
            .child(Label::new(text.clone()).text_size(FONT_SIZE_SM).text_color(muted)),
        )
        .footer(
          h_flex()
            .gap_2()
            .justify_end()
            .child(Button::new("new-branch-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
            .child(Button::new("new-branch-create").primary().label("Create Branch").on_click(move |_, window, cx| {
              let name = name.read(cx).value().to_string();
              create.update(cx, |this, cx| this.create_branch_named(&name, window, cx));
            })),
        )
    });
    let focus = input.read(cx).focus_handle(cx);
    window.defer(cx, move |window, cx| focus.focus(window, cx));
  }

  /// Creates a branch at the current commit and switches to it; the dialog closes when it worked, and otherwise says why.
  fn create_branch_named(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
    let result = self.projects.active.clone().ok_or_else(|| "Open a project first.".to_string()).and_then(|root| ws_git::create_branch(&root, name));
    match result {
      Ok(()) => {
        self.refresh_project();
        window.close_dialog(cx);
        cx.notify();
      }
      Err(message) => window.push_notification(crate::failure(message), cx),
    }
  }

  /// Opens or closes the repository menu of the title bar; opening clears the filter and focuses it.
  pub(super) fn set_projects_menu(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
    self.projects_menu_open = open;
    if open {
      self.load_project_status(true, cx);
      self.project_filter.update(cx, |input, cx| input.set_value("", window, cx));
      let focus = self.project_filter.read(cx).focus_handle(cx);
      window.defer(cx, move |window, cx| focus.focus(window, cx));
    }
    cx.notify();
  }

  /// Reads what one project holds off the interface thread, so its tab stops showing what it held before a push, pull or fetch ended
  /// while another project was open.
  pub(super) fn refresh_status_of(&mut self, root: PathBuf, cx: &mut Context<Self>) {
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move { ws_git::quick_status(&root) }
    });
    cx.spawn(async move |this, cx| {
      let found = work.await.map(|(changed, push)| ProjectStatus { changed, push });
      let _ = this.update(cx, |this, cx| {
        if job_scope::remember_status(&mut this.project_status, &root, found) {
          cx.notify();
        }
      });
    })
    .detach();
  }

  /// Settles what a background job for `root` changed, once it ended: the open project is read again; when the user moved on to
  /// another one, only what the tab of `root` shows is. Returns whether `root` is still the open project, the only case in which the
  /// job may change anything else the window shows.
  pub(super) fn settle_job(&mut self, root: &Path, cx: &mut Context<Self>) -> bool {
    let here = job_scope::lands_here(self.projects.active.as_deref(), root);
    if here {
      self.refresh_project();
    } else {
      self.refresh_status_of(root.to_path_buf(), cx);
    }
    here
  }

  /// `message`, naming the project when the job that made it ended in another project than the open one.
  pub(super) fn say_where(&self, root: &Path, message: String) -> String {
    let place = (!job_scope::lands_here(self.projects.active.as_deref(), root))
      .then(|| self.projects.projects.iter().find(|project| project.path == root).map(|project| project.name()))
      .flatten();
    job_scope::name_the_place(&message, place.as_deref())
  }

  /// Empties the commit box of `root` after a commit was made there: the open project's boxes, or the draft put away for another one.
  pub(super) fn clear_commit_draft(&mut self, root: &Path, window: &mut Window, cx: &mut Context<Self>) {
    if job_scope::lands_here(self.projects.active.as_deref(), root) {
      self.commit_summary.update(cx, |input, cx| input.set_value("", window, cx));
      self.commit_description.update(cx, |input, cx| input.set_value("", window, cx));
      self.excluded.clear();
      self.selected_change = None;
    } else {
      self.drafts.clear(root);
      if let Some(view) = self.views.get_mut(root) {
        view.selected_change = None;
      }
    }
  }

  /// The push, pull or fetch that is running for the open project, not for another one.
  pub(super) fn remote_work_here(&self) -> Option<&RemoteWork> {
    self.remote_work.as_ref().filter(|work| job_scope::shown_in(&work.root, self.projects.active.as_deref()))
  }

  /// What a project holds: for the open project what the window already read, for the others what the last background read found.
  pub(super) fn status_of(&self, path: &Path) -> Option<ProjectStatus> {
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
  pub(super) fn load_project_status(&mut self, all: bool, cx: &mut Context<Self>) {
    self.project_status_read = Some(Instant::now());
    let paths: Vec<PathBuf> = if all {
      self.projects.projects.iter().map(|project| project.path.clone()).collect()
    } else {
      self.projects.favorites().map(|project| project.path.clone()).collect()
    };
    let work = cx.background_executor().spawn(async move {
      paths
        .into_iter()
        .filter_map(|path| ws_git::quick_status(&path).map(|(changed, push)| (path, ProjectStatus { changed, push })))
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
}
