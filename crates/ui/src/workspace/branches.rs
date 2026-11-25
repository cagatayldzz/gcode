//! Branch actions of the branch menu: switching with uncommitted changes (leave them on the branch or bring
//! them along), stashed changes waiting on a branch, renaming and deleting.

use super::*;
use ws_git::Changes;

/// A button of a dialog that closes it and then does something.
fn dialog_button(id: &'static str, label: &'static str, primary: bool, action: impl Fn(&mut Window, &mut App) + 'static) -> Button {
  let button = Button::new(id).label(label);
  let button = if primary { button.primary() } else { button.outline() };
  button.on_click(move |_, window, cx| {
    window.close_dialog(cx);
    action(window, cx);
  })
}

impl Workspace {
  /// Switches the open project's repository to another branch (a remote one gets a local branch that follows it). With uncommitted
  /// changes it first asks what to do with them.
  pub(super) fn switch_branch(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
    self.branch_menu_open = false;
    cx.notify();
    let Some(git) = self.git.as_ref() else { return };
    if git.branch == name || self.switching_branch.is_some() {
      return;
    }
    if git.changes.is_empty() {
      self.run_switch(name, None, window, cx);
    } else {
      self.ask_switch_changes(name, window, cx);
    }
  }

  fn ask_switch_changes(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
    let Some(git) = self.git.as_ref() else { return };
    let (current, count) = (git.branch.clone(), git.changes.len());
    let workspace = cx.entity();
    let muted = cx.theme().muted_foreground;
    let text = SharedString::from(format!(
      "You have changes on {current} ({count} changed {}). What would you like to do with them?\n\nLeave my changes on {current}: they are stashed and wait there until you come back.\nBring my changes to {name}: they are applied on {name}.",
      if count == 1 { "file" } else { "files" },
    ));
    window.open_dialog(cx, move |dialog, _, _| {
      let (leave, bring) = (workspace.clone(), workspace.clone());
      let (leave_name, bring_name) = (name.clone(), name.clone());
      dialog.title(format!("Switch to {name}")).width(px(480.)).p_5().child(
        v_flex().gap_4().child(Label::new(text.clone()).text_color(muted)).child(
          h_flex()
            .gap_2()
            .justify_end()
            .child(dialog_button("switch-cancel", "Cancel", false, |_, _| {}))
            .child(dialog_button("switch-bring", "Bring My Changes", false, move |window, cx| {
              let name = bring_name.clone();
              bring.update(cx, |this, cx| this.run_switch(name, Some(Changes::Bring), window, cx));
            }))
            .child(dialog_button("switch-leave", "Leave My Changes", true, move |window, cx| {
              let name = leave_name.clone();
              leave.update(cx, |this, cx| this.run_switch(name, Some(Changes::Leave), window, cx));
            })),
        ),
      )
    });
  }

  fn run_switch(&mut self, name: String, changes: Option<Changes>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    self.switching_branch = Some(name.clone());
    cx.notify();
    let work = cx.background_executor().spawn({
      let (root, name) = (root.clone(), name.clone());
      async move {
        let result = match changes {
          Some(changes) => ws_git::switch_branch_with(&root, &name, changes),
          None => ws_git::switch_branch(&root, &name),
        };
        let stashed = result.is_ok() && ws_git::has_stash(&root, &name);
        (result, stashed)
      }
    });
    cx.spawn_in(window, async move |this, cx| {
      let (result, stashed) = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.switching_branch = None;
        let here = this.settle_job(&root, cx);
        match result {
          Ok(()) => {
            // The commits of the old branch are not the ones of this one: they are read again, with the loader, when History is open
            // (otherwise the next time it is opened, which finds them out of date).
            this.history_loaded = None;
            if here && this.git_view == GitView::History {
              this.load_history(cx);
            }
            // The question acts on the open project's stash: it is asked only while the open project is the one that switched.
            if stashed && here {
              this.ask_stashed_changes(name, window, cx);
            }
          }
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// The branch just switched to has changes stashed on it: restore them, throw them away, or leave them for later.
  fn ask_stashed_changes(&mut self, branch: String, window: &mut Window, cx: &mut Context<Self>) {
    let workspace = cx.entity();
    let muted = cx.theme().muted_foreground;
    window.open_dialog(cx, move |dialog, _, _| {
      let (restore, discard) = (workspace.clone(), workspace.clone());
      let (restore_branch, discard_branch) = (branch.clone(), branch.clone());
      dialog.title("Stashed changes").width(px(460.)).p_5().child(
        v_flex()
          .gap_4()
          .child(Label::new(format!("You have changes stashed on {branch}. Restore them to the working tree?")).text_color(muted))
          .child(
            h_flex()
              .gap_2()
              .justify_end()
              .child(dialog_button("stash-later", "Not Now", false, |_, _| {}))
              .child(dialog_button("stash-discard", "Discard", false, move |window, cx| {
                let branch = discard_branch.clone();
                discard.update(cx, |this, cx| this.stash_action(branch, false, window, cx));
              }))
              .child(dialog_button("stash-restore", "Restore", true, move |window, cx| {
                let branch = restore_branch.clone();
                restore.update(cx, |this, cx| this.stash_action(branch, true, window, cx));
              })),
          ),
      )
    });
  }

  fn stash_action(&mut self, branch: String, restore: bool, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let work = cx.background_executor().spawn({
      let (root, branch) = (root.clone(), branch.clone());
      async move {
        if restore {
          ws_git::restore_stash(&root, &branch)
        } else {
          ws_git::discard_stash(&root, &branch)
        }
      }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.settle_job(&root, cx);
        match result {
          Ok(()) => {
            let message = if restore { "Stashed changes restored." } else { "Stashed changes discarded." };
            window.push_notification(Notification::info(this.say_where(&root, message.to_string())), cx)
          }
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Asks for a new name for a local branch and renames it.
  pub(super) fn rename_branch(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
    self.branch_menu_open = false;
    cx.notify();
    self.ask_name(
      "Rename branch",
      "New branch name",
      "Press Enter to rename the branch.",
      &name.clone(),
      window,
      cx,
      move |this, new_name, window, cx| {
        let root = this.projects.active.clone().ok_or("Open a project first.")?;
        ws_git::rename_branch(&root, &name, new_name)?;
        this.refresh_project();
        window.push_notification(Notification::info(format!("Renamed {name} to {}.", new_name.trim())), cx);
        Ok(())
      },
    );
  }

  /// Asks before deleting a branch. A local branch that follows a remote one can be deleted there too; a branch only a remote has
  /// (`remote`) is deleted there.
  pub(super) fn confirm_delete_branch(&mut self, name: String, remote: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
    self.branch_menu_open = false;
    cx.notify();
    let Some(root) = self.projects.active.clone() else { return };
    if self.git.as_ref().is_some_and(|git| git.branch == name) {
      window.push_notification(crate::failure("The branch you are on cannot be deleted. Switch to another branch first."), cx);
      return;
    }
    let upstream = if remote.is_none() { ws_git::upstream_remote(&root, &name) } else { None };
    let workspace = cx.entity();
    let muted = cx.theme().muted_foreground;
    window.open_dialog(cx, move |dialog, _, _| {
      let (local, both, only_remote) = (workspace.clone(), workspace.clone(), workspace.clone());
      let (local_name, both_name, remote_name) = (name.clone(), name.clone(), name.clone());
      let (both_remote, only_remote_remote) = (upstream.clone().unwrap_or_default(), remote.clone().unwrap_or_default());
      let description = match (&remote, &upstream) {
        (Some(remote), _) => format!("Delete the branch {name} on {remote}? Other people lose it too."),
        (None, Some(remote)) => format!("Delete the branch {name}? It also exists on {remote}, which you can delete as well."),
        (None, None) => format!("Delete the branch {name}? Its commits stay in the repository until Git cleans up."),
      };
      let buttons = h_flex().gap_2().justify_end().child(dialog_button("branch-delete-cancel", "Cancel", false, |_, _| {}));
      let buttons = if remote.is_some() {
        buttons.child(dialog_button("branch-delete-remote", "Delete on Remote", true, move |window, cx| {
          let (name, remote) = (remote_name.clone(), only_remote_remote.clone());
          only_remote.update(cx, |this, cx| this.delete_branch(name, false, Some(remote), window, cx));
        }))
      } else {
        let buttons = buttons.child(dialog_button("branch-delete-local", "Delete", upstream.is_none(), move |window, cx| {
          let name = local_name.clone();
          local.update(cx, |this, cx| this.delete_branch(name, true, None, window, cx));
        }));
        if upstream.is_some() {
          buttons.child(dialog_button("branch-delete-both", "Delete Here and on Remote", true, move |window, cx| {
            let (name, remote) = (both_name.clone(), both_remote.clone());
            both.update(cx, |this, cx| this.delete_branch(name, true, Some(remote), window, cx));
          }))
        } else {
          buttons
        }
      };
      dialog
        .title(format!("Delete {name}?"))
        .width(px(480.))
        .p_5()
        .child(v_flex().gap_4().child(Label::new(description).text_color(muted)).child(buttons))
    });
  }

  fn delete_branch(&mut self, name: String, local: bool, remote: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let accounts = self.settings.read(cx).accounts.clone();
    let work = cx.background_executor().spawn({
      let (root, name) = (root.clone(), name.clone());
      async move {
        if local {
          ws_git::delete_branch(&root, &name)?;
        }
        if let Some(remote) = remote {
          let credentials = credentials_for_repository(&accounts, &root);
          ws_git::delete_remote_branch(&root, &remote, &name, credentials.as_ref(), |_| {}).map_err(|error| {
            if local {
              format!("Deleted {name} here, but not on the remote: {error}")
            } else {
              error
            }
          })?;
        }
        Ok::<_, String>(())
      }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.settle_job(&root, cx);
        match result {
          Ok(()) => window.push_notification(Notification::info(this.say_where(&root, format!("Deleted {name}."))), cx),
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }
}

/// Actions on the commits of the History list, the Amend command and the ignore rules.
impl Workspace {
  /// Asks before adding a commit that undoes another.
  pub(super) fn confirm_revert_commit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
    let workspace = cx.entity();
    let short: String = id.chars().take(7).collect();
    window.open_alert_dialog(cx, move |alert, _, _| {
      let (workspace, id) = (workspace.clone(), id.clone());
      alert
        .title(format!("Revert {short}?"))
        .description("A new commit is added that undoes the changes of this one. Nothing is rewritten.")
        .width(px(430.))
        .confirm()
        .ok_text("Revert")
        .on_ok(move |_, window, cx| {
          workspace.update(cx, |this, cx| this.revert_commit(id.clone(), window, cx));
          true
        })
    });
  }

  fn revert_commit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
    self.run_commit_action("Reverted", id, window, cx, ws_git::revert_commit);
  }

  /// Copies a commit onto another branch and leaves that branch checked out.
  pub(super) fn cherry_pick_commit(&mut self, id: String, branch: String, window: &mut Window, cx: &mut Context<Self>) {
    self.run_commit_action(&format!("Cherry-picked onto {branch} as"), id, window, cx, move |root, id| ws_git::cherry_pick(root, id, &branch));
  }

  fn run_commit_action(
    &mut self,
    done: &str,
    id: String,
    window: &mut Window,
    cx: &mut Context<Self>,
    action: impl FnOnce(&Path, &str) -> Result<String, String> + Send + 'static,
  ) {
    let Some(root) = self.projects.active.clone() else { return };
    let done = done.to_string();
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move { action(&root, &id) }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        let here = this.settle_job(&root, cx);
        match result {
          Ok(short) => {
            if here {
              this.load_history(cx);
            }
            window.push_notification(Notification::info(this.say_where(&root, format!("{done} {short}."))), cx);
          }
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Asks for a name and creates a branch at a commit.
  pub(super) fn branch_from_commit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
    let short: String = id.chars().take(7).collect();
    self.ask_name(
      format!("New branch at {short}"),
      "Branch name",
      "Press Enter to create the branch and switch to it.",
      "",
      window,
      cx,
      move |this, name, _, cx| {
        let root = this.projects.active.clone().ok_or("Open a project first.")?;
        ws_git::branch_from_commit(&root, &id, name)?;
        this.refresh_project();
        this.load_history(cx);
        Ok(())
      },
    );
  }

  /// Replaces the last commit with one that also has the ticked files and the message in the commit box. With an empty summary the
  /// message of the last commit is kept.
  pub(super) fn amend_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (Some(root), Some(git)) = (self.projects.active.clone(), self.git.as_ref()) else { return };
    if self.committing || self.discarding || git.merging || self.merge_task.is_some() {
      return;
    }
    let files: Vec<PathBuf> = git.changes.iter().filter(|change| !self.excluded.contains(&change.path)).map(|change| change.path.clone()).collect();
    let mut summary = self.commit_summary.read(cx).value().to_string();
    let mut description = self.commit_description.read(cx).value().to_string();
    self.committing = true;
    cx.notify();
    let work = cx.background_executor().spawn({
      let (root, files) = (root.clone(), files.clone());
      async move {
        if summary.trim().is_empty() {
          (summary, description) = ws_git::head_message(&root).ok_or("There is no commit to amend.")?;
        }
        ws_git::amend_commit(&root, &files, &summary, &description)
      }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.committing = false;
        match result {
          Ok(_) => {
            this.clear_commit_draft(&root, window, cx);
            if this.settle_job(&root, cx) {
              this.load_history(cx);
            }
          }
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Adds rules to the repository's `.gitignore`.
  pub(super) fn ignore_patterns(&mut self, patterns: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    match ws_git::add_to_gitignore(&root, &patterns) {
      Ok(()) => {
        self.refresh_project();
        window.push_notification(Notification::info(format!("Added {} to .gitignore.", patterns.join(", "))), cx);
      }
      Err(message) => window.push_notification(crate::failure(message), cx),
    }
    cx.notify();
  }
}

impl Workspace {
  /// Adds a `Co-authored-by` line for a person to the description of the commit box, once.
  pub(super) fn add_co_author(&mut self, name: &str, email: &str, window: &mut Window, cx: &mut Context<Self>) {
    let line = format!("Co-authored-by: {name} <{email}>");
    let description = self.commit_description.read(cx).value().to_string();
    if description.lines().any(|existing| existing.trim().eq_ignore_ascii_case(&line)) {
      return;
    }
    let mut next = description.trim_end().to_string();
    if !next.is_empty() {
      next.push_str("\n\n");
    }
    next.push_str(&line);
    self.commit_description.update(cx, |input, cx| input.set_value(next, window, cx));
  }
}
