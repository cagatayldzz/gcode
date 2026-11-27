//! The worktree manager for the active repository.

use super::*;
use ws_git::Worktree;

const WORKTREE_DIALOG_WIDTH: Pixels = px(600.);

pub(super) struct WorktreeList {
  root: PathBuf,
  current: PathBuf,
  trees: Vec<Worktree>,
}

impl WorktreeList {
  fn read(root: PathBuf) -> Result<Self, String> {
    let current = ws_git::workdir_file(&root, Path::new(""))
      .ok_or("Open a Git working folder to manage its worktrees.")?
      .canonicalize()
      .map_err(|error| error.to_string())?;
    let trees = ws_git::worktrees(&root)?;
    Ok(Self { root, current, trees })
  }
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = app, no_json)]
pub(super) struct ManageWorktree {
  root: PathBuf,
  operation: WorktreeAction,
}

#[derive(Clone, PartialEq)]
enum WorktreeAction {
  Open(PathBuf),
  Add { path: PathBuf, branch: String },
  Remove(Worktree),
}

impl Workspace {
  pub(super) fn show_worktrees(&mut self, _: &crate::ShowWorktrees, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else {
      window.push_notification(crate::failure("Open a Git project to see its worktrees."), cx);
      return;
    };
    if self.worktree_task.is_some() {
      return;
    }
    self.branch_menu_open = false;
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move { WorktreeList::read(root) }
    });
    self.worktree_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.worktree_task = None;
        if this.projects.active.as_ref() != Some(&root) {
          return;
        }
        match result {
          Ok(list) => {
            this.worktree_list = Some(list);
            this.render_worktree_dialog(root, window, cx);
          }
          Err(error) => window.push_notification(crate::failure(error), cx),
        }
        cx.notify();
      });
    }));
    cx.notify();
  }

  fn render_worktree_dialog(&self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let branch = cx.new(|cx| InputState::new(window, cx).placeholder("Local branch name"));
    let path = cx.new(|cx| InputState::new(window, cx).placeholder("Absolute path to a new folder"));
    let workspace = cx.entity().downgrade();
    let focus = self.focus_handle.clone();
    window.open_dialog(cx, move |dialog, _, cx| {
      let Some(workspace) = workspace.upgrade() else { return dialog };
      let this = workspace.read(cx);
      let Some(list) = this.worktree_list.as_ref().filter(|list| this.projects.active.as_ref() == Some(&list.root) && list.root == root) else {
        return dialog.title("Worktrees").child(Label::new("Open the worktree manager again for the current project."));
      };
      let busy = this.worktree_task.is_some();
      let muted = cx.theme().muted_foreground;
      let rows = list.trees.iter().map(|tree| {
        let current = tree.path == list.current;
        let status = match (current, tree.main, tree.locked, tree.prunable) {
          (true, _, _, _) => "Current",
          (_, true, _, _) => "Main",
          (_, _, true, _) => "Locked",
          (_, _, _, true) => "Unavailable",
          _ => "Linked",
        };
        let open = ManageWorktree { root: root.clone(), operation: WorktreeAction::Open(tree.path.clone()) };
        let remove = ManageWorktree { root: root.clone(), operation: WorktreeAction::Remove(tree.clone()) };
        let (open_focus, remove_focus) = (focus.clone(), focus.clone());
        h_flex()
          .items_center()
          .gap_2()
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .child(Label::new(format!("{} · {status}", tree.branch.as_deref().unwrap_or("Detached HEAD"))).truncate())
              .child(Label::new(tree.path.display().to_string()).text_sm().text_color(muted).truncate()),
          )
          .child(
            Button::new(SharedString::from(format!("open-worktree-{}", tree.path.display())))
              .small()
              .label("Open Project")
              .disabled(busy || tree.prunable || current)
              .on_click(move |_, window, cx| open_focus.dispatch_action(&open, window, cx)),
          )
          .when(!current && !tree.main && !tree.locked && !tree.prunable, |row| {
            row.child(
              Button::new(SharedString::from(format!("remove-worktree-{}", tree.path.display())))
                .small()
                .outline()
                .label("Remove…")
                .disabled(busy)
                .on_click(move |_, window, cx| remove_focus.dispatch_action(&remove, window, cx)),
            )
          })
      });
      let (branch, path, focus, root) = (branch.clone(), path.clone(), focus.clone(), root.clone());
      dialog.title("Worktrees").width(WORKTREE_DIALOG_WIDTH).child(
        v_flex().gap_3().child(v_flex().gap_2().children(rows).max_h_64().overflow_y_scrollbar()).child(
          v_flex()
            .gap_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .pt_3()
            .child(Label::new("Add Worktree").font_weight(FontWeight::SEMIBOLD))
            .child(Label::new("Choose an existing local branch or enter a new branch name to start from HEAD.").text_sm().text_color(muted))
            .child(crate::components::soft_input(&branch, cx))
            .child(crate::components::soft_input(&path, cx))
            .child(Button::new("add-worktree").primary().label("Add Worktree").disabled(busy).on_click(move |_, window, cx| {
              let operation =
                WorktreeAction::Add { path: PathBuf::from(path.read(cx).value().trim()), branch: branch.read(cx).value().trim().to_owned() };
              focus.dispatch_action(&ManageWorktree { root: root.clone(), operation }, window, cx);
            })),
        ),
      )
    });
  }

  pub(super) fn manage_worktree(&mut self, action: &ManageWorktree, window: &mut Window, cx: &mut Context<Self>) {
    if self.projects.active.as_ref() != Some(&action.root) || self.worktree_task.is_some() {
      return;
    }
    match &action.operation {
      WorktreeAction::Open(path) => {
        if !path.is_dir() {
          window.push_notification(crate::failure("The worktree folder is unavailable."), cx);
          return;
        }
        window.close_dialog(cx);
        let path = path.clone();
        self.switch_project(window, cx, move |projects| {
          projects.add(path.clone());
          projects.set_favorite(&path, true);
        });
      }
      WorktreeAction::Add { .. } => self.change_worktree(action.clone(), window, cx),
      WorktreeAction::Remove(tree) => {
        let action = action.clone();
        let title: SharedString = format!("Remove {}?", tree.path.file_name().unwrap_or(tree.path.as_os_str()).to_string_lossy()).into();
        let description: SharedString =
          format!("Delete the working folder at {} and keep its branch. Git must find the worktree clean.", tree.path.display()).into();
        let workspace = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
          let (workspace, action) = (workspace.clone(), action.clone());
          alert.title(title.clone()).description(description.clone()).confirm().ok_text("Remove").ok_variant(ButtonVariant::Danger).on_ok(
            move |_, window, cx| {
              let _ = workspace.update(cx, |this, cx| this.change_worktree(action.clone(), window, cx));
              true
            },
          )
        });
      }
    }
  }

  fn change_worktree(&mut self, action: ManageWorktree, window: &mut Window, cx: &mut Context<Self>) {
    let ManageWorktree { root, operation } = action;
    if self.projects.active.as_ref() != Some(&root) || self.worktree_task.is_some() {
      return;
    }
    if let WorktreeAction::Remove(tree) = &operation {
      if self.agent_worktree_running(&tree.path) {
        window.push_notification(crate::failure("Close the agent session before removing its worktree."), cx);
        return;
      }
      if self.has_pending_editor_changes(&tree.path, cx) {
        window.push_notification(crate::failure("Save the affected editor files before removing the worktree."), cx);
        return;
      }
    }
    let removed = match &operation {
      WorktreeAction::Remove(tree) => Some(tree.path.clone()),
      _ => None,
    };
    let work = cx.background_executor().spawn({
      let root = root.clone();
      async move {
        let changed = match operation {
          WorktreeAction::Add { path, branch } => ws_git::add_worktree(&root, &path, &branch).map(|path| format!("Created {}.", path.display())),
          WorktreeAction::Remove(tree) => ws_git::remove_worktree(&root, &tree).map(|()| format!("Removed {}.", tree.path.display())),
          WorktreeAction::Open(_) => return (Err("Choose Add Worktree or Remove.".into()), None),
        };
        // A checkout hook can fail after Git has already registered the worktree.
        (changed, Some(WorktreeList::read(root)))
      }
    });
    self.worktree_task = Some(cx.spawn_in(window, async move |this, cx| {
      let (result, list) = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.worktree_task = None;
        match result {
          Ok(message) => {
            if let Some(path) = removed {
              let projects: Vec<_> =
                this.projects.projects.iter().filter(|project| project.path.starts_with(&path)).map(|project| project.path.clone()).collect();
              for project in projects {
                this.remove_project(&project, window, cx);
              }
            }
            window.push_notification(Notification::info(this.say_where(&root, message)), cx);
          }
          Err(error) => window.push_notification(crate::failure(this.say_where(&root, error)), cx),
        }
        if this.settle_job(&root, cx) {
          match list {
            Some(Ok(list)) => this.worktree_list = Some(list),
            Some(Err(error)) => {
              this.worktree_list = None;
              window.push_notification(crate::failure(error), cx);
            }
            None => {}
          }
        }
        cx.notify();
        window.refresh();
      });
    }));
    cx.notify();
    window.refresh();
  }
}
