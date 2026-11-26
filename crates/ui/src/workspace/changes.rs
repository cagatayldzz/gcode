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
    self.editor_panel.update(cx, |editor, cx| editor.reveal_position(&path, line.saturating_sub(1) as usize, 0, window, cx));
    window.focus(&self.editor_panel.focus_handle(cx), cx);
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
            this.editor_panel.update(cx, |editor, cx| {
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

  /// Commits the changed files that are ticked, with the summary and description typed below the list.
  pub(super) fn commit_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (Some(root), Some(git)) = (self.projects.active.clone(), self.git.as_ref()) else { return };
    if self.committing || self.discarding || git.operation != ws_git::Operation::Idle || self.merge_task.is_some() {
      return;
    }
    let files: Vec<PathBuf> = git.changes.iter().filter(|change| !self.excluded.contains(&change.path)).map(|change| change.path.clone()).collect();
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
        let id = ws_git::commit(&root, &files, &summary, &description)?;
        let pushed = push.then(|| ws_git::push(&root, credentials_for_repository(&accounts, &root).as_ref(), |_| {}));
        Ok::<_, String>((id, pushed))
      }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.committing = false;
        match result {
          Ok((id, pushed)) => {
            // The diff of a file that was just committed has nothing left to show.
            if let Some(open) =
              this.selected_change.clone().filter(|open| files.contains(open) && job_scope::lands_here(this.projects.active.as_deref(), &root))
            {
              if let Some(path) = ws_git::workdir_file(&root, &open) {
                this.editor_panel.update(cx, |editor, cx| {
                  if !editor.is_dirty(&path) {
                    editor.forget(&path, cx);
                  }
                });
              }
            }
            this.clear_commit_draft(&root, window, cx);
            this.settle_job(&root, cx);
            match pushed {
              None | Some(Ok(_)) => {}
              Some(Err(message)) => {
                if let Some(kind) = refusal_kind(&message) {
                  this.flag_account_issue(&remote_url, kind, cx);
                }
                this.push_remote_failure(format!("Committed {id}, but the push failed: {message}"), &remote_url, window, cx)
              }
            }
          }
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }
}
