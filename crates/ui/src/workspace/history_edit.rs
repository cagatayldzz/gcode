//! Confirm and run explicit history rewrites for the project that opened the menu.

use super::*;
use ws_git::{HistoryHead, Operation, ResetMode};

#[derive(Action, Clone, PartialEq)]
#[action(namespace = app, no_json)]
pub(super) struct RewriteHistory {
  pub root: PathBuf,
  pub rewrite: Rewrite,
}

#[derive(Clone, PartialEq)]
pub(super) enum Rewrite {
  Reset { id: String, mode: ResetMode },
  Rebase { branch: String, remote: Option<String> },
}

impl Rewrite {
  fn confirmation(&self, head: &HistoryHead) -> (String, &'static str, &'static str) {
    let label = head.label();
    match self {
      Self::Reset { id, mode } => {
        let short: String = id.chars().take(7).collect();
        let description = match mode {
          ResetMode::Soft => {
            "Move HEAD and keep the staging area and working files. Commits removed from the branch may already have been published."
          }
          ResetMode::Mixed => "Move HEAD and reset the staging area to this commit. Keep working files and leave their changes unstaged.",
          ResetMode::Hard => {
            "Move HEAD and discard staged and unstaged tracked changes. Untracked or ignored files in the way of checkout may also be deleted."
          }
        };
        (format!("Reset {label} to {short}?"), description, "Reset")
      }
      Self::Rebase { branch, remote } => {
        let base = remote.as_ref().map_or_else(|| branch.clone(), |remote| format!("{remote}/{branch}"));
        (
          format!("Rebase {label} onto {base}?"),
          "Replay the current branch's commits on the selected branch. This can rewrite commit IDs that may already have been published.",
          "Rebase",
        )
      }
    }
  }
}

impl Workspace {
  fn git_rewrite_available(&self, root: &Path, window: &mut Window, cx: &mut Context<Self>) -> bool {
    let error = if self.projects.active.as_deref() != Some(root) {
      "The active project changed. Open the operation again."
    } else if self.merge_task.is_some()
      || self.remote_work.is_some()
      || self.committing
      || self.discarding
      || self.stage_task.is_some()
      || self.stash_task.is_some()
      || self.switching_branch.is_some()
      || self.git.as_ref().is_none_or(|git| git.operation != Operation::Idle)
    {
      "Finish the current Git operation before rewriting history."
    } else if self.has_pending_editor_changes(root, cx)
      || ws_git::workdir_file(root, Path::new("")).is_some_and(|workdir| self.has_pending_editor_changes(&workdir, cx))
    {
      "Save the affected editor files before rewriting history."
    } else {
      return true;
    };
    window.push_notification(crate::failure(error), cx);
    false
  }

  pub(super) fn has_pending_editor_changes(&self, root: &Path, cx: &App) -> bool {
    self.editor_panels().any(|panel| panel.read(cx).has_unsaved_under(root)) || self.editor_saves.keys().any(|path| path.starts_with(root))
  }

  pub(super) fn confirm_history_rewrite(&mut self, action: &RewriteHistory, window: &mut Window, cx: &mut Context<Self>) {
    if !self.git_rewrite_available(&action.root, window, cx) {
      return;
    }
    self.branch_menu_open = false;
    let action = action.clone();
    let head = match ws_git::history_head(&action.root) {
      Ok(head) => head,
      Err(error) => {
        window.push_notification(crate::failure(error), cx);
        return;
      }
    };
    let (title, description, button) = action.rewrite.confirmation(&head);
    let title: SharedString = title.into();
    let workspace = cx.entity().downgrade();
    window.open_alert_dialog(cx, move |alert, _, _| {
      let (workspace, action, head) = (workspace.clone(), action.clone(), head.clone());
      alert.title(title.clone()).description(description).confirm().ok_text(button).ok_variant(ButtonVariant::Danger).on_ok(move |_, window, cx| {
        let _ = workspace.update(cx, |this, cx| this.rewrite_history(action.clone(), head.clone(), window, cx));
        true
      })
    });
    cx.notify();
  }

  fn rewrite_history(&mut self, action: RewriteHistory, expected: HistoryHead, window: &mut Window, cx: &mut Context<Self>) {
    let RewriteHistory { root, rewrite } = action;
    if !self.git_rewrite_available(&root, window, cx) {
      return;
    }
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move {
        match rewrite {
          Rewrite::Reset { id, mode } => ws_git::reset_to_commit(&root, &expected, &id, mode).map(|()| "Reset complete."),
          Rewrite::Rebase { branch, remote } => ws_git::rebase_onto(&root, &expected, &branch, remote.as_deref()).map(|state| {
            if state == Operation::Rebase {
              "Rebase paused. Resolve and stage the conflicts, then continue, skip, or abort."
            } else {
              "Rebase complete."
            }
          }),
        }
      }
    });
    self.merge_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.merge_task = None;
        if this.settle_job(&root, cx) {
          this.load_history(cx);
        }
        match result {
          Ok(message) => window.push_notification(Notification::info(this.say_where(&root, message.into())), cx),
          Err(error) => window.push_notification(crate::failure(this.say_where(&root, error)), cx),
        }
        cx.notify();
      });
    }));
    cx.notify();
  }
}
