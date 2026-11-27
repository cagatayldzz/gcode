//! Changed files: viewing, discarding and committing.

use super::*;

impl Workspace {
  pub(super) fn toggle_all_changes(&mut self, _: &ToggleAllChanges, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    self.all_changes_open = !self.all_changes_open;
    if self.all_changes_open {
      self.selected_change = None;
      self.hide_terminal(window, cx);
      self.all_changes.update(cx, |view, cx| view.reload(root, cx));
    }
    cx.notify();
  }

  /// Shows the changes of a changed file in the main area, in place of the editor. Choosing another file replaces them.
  pub(super) fn open_change(&mut self, file: PathBuf, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    if self.all_changes_open {
      // The list on the left is the table of contents of the all-changes view: choosing a file scrolls to it.
      let path = file.to_string_lossy().into_owned();
      self.all_changes.update(cx, |view, cx| view.scroll_to(&path, cx));
      self.selected_change = Some(file);
      cx.notify();
      return;
    }
    self.all_changes_open = false;
    // Choosing the file already shown leaves its diff (and scroll position) alone.
    if self.selected_change.as_ref() != Some(&file) {
      let change = self.change_of(&file);
      self.change_view.update(cx, |view, cx| view.show(root, file.clone(), change.as_ref(), cx));
    }
    self.selected_change = Some(file);
    self.selected_entry = None;
    cx.notify();
  }

  /// The change of a changed file: its kind, and where it was before when it was renamed.
  pub(super) fn change_of(&self, file: &Path) -> Option<ws_git::Change> {
    self.git.as_ref()?.changes.iter().find(|change| change.path == file).cloned()
  }

  /// Opens a changed file itself, not its diff, in the editor.
  pub(crate) fn open_change_in_editor(&mut self, file: &Path, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    match ws_git::workdir_file(&root, file).filter(|path| path.is_file()) {
      Some(path) => self.reveal_file(root, path, window, cx),
      None => window.push_notification(crate::failure(format!("{} is not on the disk.", file.display())), cx),
    }
  }

  /// Opens a changed file in the editor with the cursor on `line` (counted from 1), and scrolls there.
  pub(super) fn open_change_line_in_editor(&mut self, file: &Path, line: u32, window: &mut Window, cx: &mut Context<Self>) {
    self.open_change_in_editor(file, window, cx);
    let Some(path) = self.projects.active.as_deref().and_then(|root| Self::changed_file_path(root, &file.to_string_lossy())) else { return };
    self.editor_for_path(&path, cx).update(cx, |editor, cx| editor.reveal_position(&path, line.saturating_sub(1) as usize, 0, window, cx));
    self.editor_for_path(&path, cx).update(cx, |editor, cx| editor.focus_document(window, cx));
  }

  pub(super) fn show_change_in_finder(root: PathBuf, file: PathBuf, window: &mut Window, cx: &mut App) {
    let work = cx.background_executor().spawn(async move {
      let path = ws_git::workdir_file(&root, &file).ok_or("The repository's working folder is unavailable.")?;
      let mut command = std::process::Command::new("/usr/bin/open");
      if path.exists() {
        command.arg("-R").arg(path);
      } else {
        let folder = path.parent().and_then(|parent| parent.ancestors().find(|folder| folder.is_dir()));
        command.arg(folder.ok_or("No containing folder is available.")?);
      }
      let result = command.output().map_err(|error| error.to_string())?;
      if result.status.success() {
        return Ok(());
      }
      let error = String::from_utf8_lossy(&result.stderr);
      Err(if error.trim().is_empty() { result.status.to_string() } else { error.trim().to_owned() })
    });
    window
      .spawn(cx, async move |cx| {
        if let Err(error) = work.await {
          let _ = cx.update(|window, cx| window.push_notification(crate::failure(format!("Could not show the file in Finder. {error}")), cx));
        }
      })
      .detach();
  }

  /// Confirms the exact set of files before starting an irreversible discard.
  pub(super) fn confirm_discard_changes(&self, files: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    if files.is_empty() || self.committing || self.discarding {
      return;
    }
    if let Some(operation) = self.git.as_ref().map(|git| git.operation).filter(|operation| *operation != ws_git::Operation::Idle) {
      self.explain_discard_blocked(operation, window, cx);
      return;
    }
    let count = files.len();
    let (title, description) = if let [file] = files.as_slice() {
      (
        format!("Discard changes to {}?", file.display()),
        "Staged and unstaged changes will be lost. New files will be deleted. This cannot be undone.".to_owned(),
      )
    } else {
      let mut names = files.iter().take(5).map(|file| file.display().to_string()).collect::<Vec<_>>().join("\n");
      if count > 5 {
        names.push_str(&format!("\n… and {} more", count - 5));
      }
      (
        format!("Discard changes to {count} files?"),
        format!("Staged and unstaged changes will be lost. New files will be deleted. This cannot be undone.\n\n{names}"),
      )
    };
    let workspace = cx.entity();
    let (title, description) = (SharedString::from(title), SharedString::from(description));
    window.open_alert_dialog(cx, move |alert, _, _| {
      let (workspace, root, files) = (workspace.clone(), root.clone(), files.clone());
      alert
        .title(title.clone())
        .description(description.clone())
        .width(px(430.))
        .confirm()
        .ok_text("Discard")
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, window, cx| {
          workspace.update(cx, |this, cx| this.discard_changes(root.clone(), files.clone(), window, cx));
          true
        })
    });
  }

  /// Files cannot be discarded one by one while a Git operation runs: restoring them would silently drop what the operation
  /// brought in. Offers the way out instead: the merge resolver (which can abort) or aborting the operation.
  fn explain_discard_blocked(&self, operation: ws_git::Operation, window: &mut Window, cx: &mut Context<Self>) {
    let is_merge = operation == ws_git::Operation::Merge;
    let title = SharedString::from(format!("{} in progress", operation.label()));
    let description = SharedString::from(if is_merge {
      "Changes cannot be discarded while a merge is in progress. To get rid of them, abort the merge: the repository returns to its state before the merge."
    } else if operation.can_resume() {
      "Changes cannot be discarded while this operation is in progress. Abort it to return to the state before it."
    } else {
      "Changes cannot be discarded while this operation is in progress. Finish it in the terminal first."
    });
    let can_act = is_merge || operation.can_resume();
    let workspace = cx.entity().downgrade();
    window.open_alert_dialog(cx, move |alert, _, _| {
      let workspace = workspace.clone();
      let alert = alert.title(title.clone()).description(description.clone());
      if !can_act {
        return alert;
      }
      alert.confirm().ok_text(if is_merge { "Open merge…" } else { "Abort…" }).on_ok(move |_, window, cx| {
        let _ = workspace.update(cx, |this, cx| {
          if is_merge {
            this.merge_compare(window, cx);
          } else {
            this.abort_git_operation(&crate::AbortGitOperation, window, cx);
          }
        });
        true
      })
    });
  }

  pub(super) fn discard_changes(&mut self, root: PathBuf, files: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
    if self.committing || self.discarding || self.projects.active.as_ref() != Some(&root) {
      return;
    }
    self.discarding = true;
    let work_root = root.clone();
    let work = cx.background_executor().spawn(async move {
      let mut discarded = Vec::new();
      let mut failures = Vec::new();
      for file in files {
        match ws_git::discard_changes(&work_root, &file) {
          Ok(()) => discarded.push((ws_git::workdir_file(&work_root, &file), file)),
          Err(error) => failures.push(format!("{}: {error}", file.display())),
        }
      }
      (discarded, failures)
    });
    cx.spawn_in(window, async move |this, cx| {
      let (discarded, failures) = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.discarding = false;
        let active = this.projects.active.as_ref() == Some(&root);
        for (path, file) in &discarded {
          if let Some(path) = path {
            this.editor_for_path(path, cx).update(cx, |editor, cx| {
              if !editor.is_dirty(path) {
                editor.forget(path, cx);
              }
            });
          }
          if active {
            this.excluded.remove(file);
            if this.selected_change.as_ref() == Some(file) {
              this.selected_change = None;
            }
          }
        }
        if active {
          this.refresh_in_background(cx);
        }
        let count = discarded.len();
        if let Some(error) = failures.first() {
          window.push_notification(crate::failure(format!("Discarded {count} of {} files. {error}", count + failures.len())), cx);
        } else {
          let message = if count == 1 { "Discarded changes to 1 file.".to_owned() } else { format!("Discarded changes to {count} files.") };
          window.push_notification(Notification::info(message), cx);
        }
        cx.notify();
      });
    })
    .detach();
    cx.notify();
  }

  pub(super) fn selected_commit_files(&self, git: &ws_git::Summary) -> Vec<PathBuf> {
    git
      .changes
      .iter()
      .filter(|change| !self.excluded.contains(&change.path))
      .flat_map(|change| std::iter::once(change.path.clone()).chain(change.from.clone()))
      .collect()
  }

  pub(super) fn stage_change(&mut self, request: &crate::change_view::StageRequest, window: &mut Window, cx: &mut Context<Self>) {
    if self.projects.active.as_ref() != Some(&request.root)
      || self.stage_task.is_some()
      || self.committing
      || self.discarding
      || self.merge_task.is_some()
      || self.switching_branch.is_some()
      || self.remote_work.is_some()
      || self.stash_task.is_some()
    {
      return;
    }
    let (root, files, side, hunk) = (request.root.clone(), request.files.clone(), request.side, request.hunk.clone());
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move { ws_git::change_index_hunks(&root, &files, side, hunk.as_ref().map(|(diff, positions)| (diff.as_ref(), positions.as_slice()))) }
    });
    self.stage_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.stage_task = None;
        if this.settle_job(&root, cx) {
          this.change_view.update(cx, |view, cx| view.reload(cx));
        }
        if let Err(message) = result {
          window.push_notification(crate::failure(this.say_where(&root, message)), cx);
        }
        cx.notify();
      });
    }));
    cx.notify();
  }

  pub(super) fn commit_staged_changes(&mut self, _: &crate::CommitStagedChanges, window: &mut Window, cx: &mut Context<Self>) {
    self.run_commit(true, window, cx);
  }

  /// Commits the changed files that are ticked, with the summary and description typed below the list.
  pub(super) fn commit_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.run_commit(false, window, cx);
  }

  fn run_commit(&mut self, staged: bool, window: &mut Window, cx: &mut Context<Self>) {
    let (Some(root), Some(git)) = (self.projects.active.clone(), self.git.as_ref()) else { return };
    if self.committing || self.discarding || self.stage_task.is_some() || git.operation != ws_git::Operation::Idle || self.merge_task.is_some() {
      return;
    }
    if staged && !git.changes.iter().any(|change| change.staged) {
      return;
    }
    let files = if staged { Vec::new() } else { self.selected_commit_files(git) };
    let summary = self.commit_summary.read(cx).value().to_string();
    let description = self.commit_description.read(cx).value().to_string();
    self.committing = true;
    cx.notify();
    let (push, accounts) = {
      let model = self.settings.read(cx);
      (model.commitgen.git_push, model.accounts.clone())
    };
    let remote_url = self.git.as_ref().and_then(|git| git.sync.as_ref()).map(|sync| sync.url.clone()).unwrap_or_default();
    let work = cx.background_executor().spawn({
      let (root, files) = (root.clone(), files.clone());
      async move {
        let commit =
          if staged { ws_git::commit_staged(&root, &summary, &description)? } else { ws_git::commit(&root, &files, &summary, &description)? };
        let pushed = (push && commit.warning.is_none()).then(|| ws_git::push(&root, credentials_for_repository(&accounts, &root).as_ref(), |_| {}));
        Ok::<_, String>((commit, pushed))
      }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.committing = false;
        match result {
          Ok((commit, pushed)) => {
            // The diff of a file that was just committed has nothing left to show.
            if let Some(open) =
              this.selected_change.clone().filter(|open| files.contains(open) && job_scope::lands_here(this.projects.active.as_deref(), &root))
            {
              if let Some(path) = ws_git::workdir_file(&root, &open) {
                this.editor_for_path(&path, cx).update(cx, |editor, cx| {
                  if !editor.is_dirty(&path) {
                    editor.forget(&path, cx);
                  }
                });
              }
            }
            this.clear_commit_draft(&root, window, cx);
            this.settle_job(&root, cx);
            if let Some(warning) = commit.warning {
              window.push_notification(crate::failure(this.say_where(&root, warning)), cx);
            }
            match pushed {
              None | Some(Ok(_)) => {}
              Some(Err(message)) => {
                if let Some(kind) = refusal_kind(&message) {
                  this.flag_account_issue(&remote_url, kind, cx);
                }
                this.push_remote_failure(format!("Committed {}, but the push failed: {message}", commit.id), &remote_url, window, cx)
              }
            }
          }
          Err(message) => {
            this.settle_job(&root, cx);
            window.push_notification(crate::failure(this.say_where(&root, message)), cx);
          }
        }
        cx.notify();
      });
    })
    .detach();
  }
}
