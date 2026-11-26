//! Branch actions of the branch menu: switching with uncommitted changes (leave them on the branch or bring
//! them along), stashed changes waiting on a branch, renaming and deleting.

use super::*;
use gpui_kit::component::radio::Radio;
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
      let target = name.clone();
      self.ask_changes(
        "Switch Branch",
        "Switch Branch",
        name,
        move |this, changes, window, cx| this.run_switch(target.clone(), Some(changes), window, cx),
        window,
        cx,
      );
    }
  }

  /// Creates a branch and switches to it. With uncommitted changes it first asks what to do with them: they come along, or stay
  /// stashed on the branch being left.
  pub(super) fn create_branch_asking(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else {
      window.push_notification(crate::failure("Open a project first."), cx);
      return;
    };
    // A name that cannot be used is reported before anything is asked.
    if let Err(message) = ws_git::check_new_branch(&root, name.trim()) {
      window.push_notification(crate::failure(message), cx);
      return;
    }
    window.close_dialog(cx);
    if self.git.as_ref().is_some_and(|git| git.changes.is_empty()) {
      self.run_create(name, None, window, cx);
    } else {
      let target = name.clone();
      self.ask_changes(
        "Create a Branch",
        "Create Branch",
        name,
        move |this, changes, window, cx| this.run_create(target.clone(), Some(changes), window, cx),
        window,
        cx,
      );
    }
  }

  fn run_create(&mut self, name: String, changes: Option<Changes>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move {
        match changes {
          Some(changes) => ws_git::create_branch_with(&root, &name, changes),
          None => ws_git::create_branch(&root, &name),
        }
      }
    });
    cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.settle_job(&root, cx);
        if let Err(message) = result {
          window.push_notification(crate::failure(message), cx);
        }
        cx.notify();
      });
    })
    .detach();
  }

  /// Asks what to do with the uncommitted changes before moving to the branch `name` (an existing one, or one about to be made):
  /// leave them on the current branch, or bring them along. `run` carries out the choice.
  fn ask_changes(
    &mut self,
    title: &'static str,
    confirm: &'static str,
    name: String,
    run: impl Fn(&mut Self, Changes, &mut Window, &mut Context<Self>) + 'static,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let Some(git) = self.git.as_ref() else { return };
    let current = git.branch.clone();
    let run = Rc::new(run);
    let workspace = cx.entity();
    // What the radio choice is now: it lives outside the dialog, which is built again every time it is drawn.
    let choice = Rc::new(Cell::new(Changes::Leave));
    window.open_dialog(cx, move |dialog, _, cx| {
      let palette = colors(cx);
      let option = |id: &'static str, value: Changes, title: String, detail: &'static str| {
        let chosen = choice.get() == value;
        let (choice, radio_choice) = (choice.clone(), choice.clone());
        h_flex()
          .id(id)
          .gap_3()
          .px_3()
          .py_2p5()
          .items_start()
          .cursor_pointer()
          .when(chosen, |row| row.bg(palette.surface))
          .child(div().pt_0p5().child(Radio::new(SharedString::from(format!("{id}-radio"))).checked(chosen).on_click(move |_, window, _| {
            radio_choice.set(value);
            window.refresh();
          })))
          .child(
            v_flex()
              .gap_0p5()
              .child(Label::new(title).font_weight(FontWeight::MEDIUM))
              .child(Label::new(detail).text_color(palette.text_secondary).text_size(FONT_SIZE_SM)),
          )
          .on_click(move |_, window, _| {
            choice.set(value);
            window.refresh();
          })
      };
      let (switch_workspace, switch_run, switch_choice) = (workspace.clone(), run.clone(), choice.clone());
      dialog
        .title(title)
        .width(px(480.))
        .p_0()
        .child(
          v_flex()
            .gap_4()
            .px_5()
            .pt_2()
            .pb_5()
            .child(Label::new("You have changes on this branch. What would you like to do with them?").text_color(palette.text_secondary))
            .child(
              v_flex()
                .rounded_md()
                .border_1()
                .border_color(palette.border)
                .overflow_hidden()
                .child(option(
                  "switch-leave-option",
                  Changes::Leave,
                  format!("Leave my changes on {current}"),
                  "Your in-progress work will be stashed on this branch for you to return to later",
                ))
                .child(div().h_px().bg(palette.border))
                .child(option(
                  "switch-bring-option",
                  Changes::Bring,
                  format!("Bring my changes to {name}"),
                  "Your in-progress work will follow you to the new branch",
                )),
            ),
        )
        .child(
          h_flex()
            .gap_2()
            .px_5()
            .py_3()
            .justify_end()
            .border_t_1()
            .border_color(palette.border)
            .child(dialog_button("switch-cancel", "Cancel", false, |_, _| {}))
            .child(dialog_button("switch-confirm", confirm, true, move |window, cx| {
              let changes = switch_choice.get();
              let run = switch_run.clone();
              switch_workspace.update(cx, |this, cx| run(this, changes, window, cx));
            })),
        )
    });
  }

  fn run_switch(&mut self, name: String, changes: Option<Changes>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    if self.stash_task.is_some() {
      return;
    }
    self.switching_branch = Some(name.clone());
    cx.notify();
    let work = cx.background_executor().spawn({
      let (root, name) = (root.clone(), name.clone());
      async move {
        let result = match changes {
          Some(changes) => ws_git::switch_branch_with(&root, &name, changes),
          None => ws_git::switch_branch(&root, &name),
        };
        let stashed = if result.is_ok() { ws_git::stash_ids(&root, &name) } else { Ok(Vec::new()) };
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
            if here {
              match stashed {
                Ok(ids) if !ids.is_empty() => this.ask_stashed_changes(root.clone(), name, ids, window, cx),
                Ok(_) => {}
                Err(message) => window.push_notification(crate::failure(message), cx),
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

  pub(super) fn show_branch_stashes(&mut self, _: &crate::ShowBranchStashes, window: &mut Window, cx: &mut Context<Self>) {
    if self.stash_task.is_some() || self.switching_branch.is_some() {
      return;
    }
    let (Some(root), Some(summary)) = (self.projects.active.clone(), self.git.as_ref()) else { return };
    let branch = summary.branch.clone();
    let work = cx.background_executor().spawn({
      let (root, branch) = (root.clone(), branch.clone());
      async move { ws_git::stash_ids(&root, &branch) }
    });
    self.stash_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.stash_task = None;
        if this.projects.active.as_ref() != Some(&root) || this.git.as_ref().is_none_or(|summary| summary.branch != branch) {
          return;
        }
        match result {
          Ok(ids) if ids.is_empty() => window.push_notification(Notification::info("No stashed changes on this branch."), cx),
          Ok(ids) => this.ask_stashed_changes(root, branch, ids, window, cx),
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
      });
    }));
  }

  fn ask_stashed_changes(&self, root: PathBuf, branch: String, ids: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
    use gpui_kit::component::dialog::DialogFooter;
    use gpui_kit::component::scroll::ScrollableElement as _;

    let workspace = cx.entity().downgrade();
    let description: SharedString = format!("Stashes on {branch}, newest first.").into();
    window.open_alert_dialog(cx, move |dialog, _, _| {
      let rows = ids.iter().map(|id| {
        let button = |label, restore| {
          let (workspace, root, branch, stash) = (workspace.clone(), root.clone(), branch.clone(), id.clone());
          Button::new(SharedString::from(format!("stash-{label}-{id}"))).small().label(label).on_click(move |_, window, cx| {
            window.close_dialog(cx);
            let _ = workspace.update(cx, |this, cx| this.stash_action(root.clone(), branch.clone(), stash.clone(), restore, window, cx));
          })
        };
        h_flex()
          .gap_2()
          .items_center()
          .child(Label::new(SharedString::from(id.chars().take(12).collect::<String>())).flex_1())
          .child(button("Restore", true))
          .child(button("Discard", false))
      });
      dialog
        .title("Stashed changes")
        .description(description.clone())
        .confirm()
        .child(v_flex().max_h_64().gap_2().children(rows).overflow_y_scrollbar())
        .footer(DialogFooter::new().child(Button::new("stash-close").label("Close").on_click(|_, window, cx| window.close_dialog(cx))))
    });
  }

  fn stash_action(&mut self, root: PathBuf, branch: String, id: String, restore: bool, window: &mut Window, cx: &mut Context<Self>) {
    if self.projects.active.as_ref() != Some(&root) || self.stash_task.is_some() {
      return;
    }
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move {
        if restore {
          if ws_git::summary(&root).is_none_or(|summary| summary.branch != branch) {
            return Err("Switch back to the stash's branch before restoring it.".into());
          }
          ws_git::restore_stash_by_id(&root, &branch, &id)
        } else {
          ws_git::discard_stash_by_id(&root, &branch, &id)
        }
      }
    });
    self.stash_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.stash_task = None;
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
    }));
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
    if self.committing || self.discarding || git.operation != ws_git::Operation::Idle || self.merge_task.is_some() {
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
