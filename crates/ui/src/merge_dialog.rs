//! The file-by-file controls for a pending Git merge.

use crate::{all_changes::OpenChangedFile, theme::colors, ToggleTerminal};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    label::Label,
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
  },
  *,
};
use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
};
use vekro_git::{MergeSide, MergeState};

pub(crate) enum MergeEvent {
  Completed,
  Aborted,
}

enum Operation {
  Refresh,
  Choose(String, MergeSide),
  Undo(String),
  Finish(bool),
}

pub(crate) struct MergeDialog {
  root: PathBuf,
  source: SharedString,
  state: MergeState,
  paths: BTreeMap<String, Option<MergeSide>>,
  task: Option<Task<()>>,
}

impl EventEmitter<MergeEvent> for MergeDialog {}

impl MergeDialog {
  pub fn new(root: PathBuf, source: SharedString, state: MergeState) -> Self {
    let paths = state.files.iter().map(|file| (file.path.clone(), None)).collect();
    Self { root, source, state, paths, task: None }
  }

  pub fn source(&self) -> &SharedString {
    &self.source
  }

  pub fn same_merge(&self, state: &MergeState) -> bool {
    self.state.same_merge(state)
  }

  pub fn set_state(&mut self, state: MergeState, cx: &mut Context<Self>) {
    for file in &state.files {
      self.paths.insert(file.path.clone(), None);
    }
    self.state = state;
    cx.notify();
  }

  pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.run(Operation::Refresh, window, cx);
  }

  fn run(&mut self, operation: Operation, window: &mut Window, cx: &mut Context<Self>) {
    if self.task.is_some() {
      return;
    }
    let (state, root) = (self.state.clone(), self.root.clone());
    let work = cx.background_executor().spawn(async move {
      let result = match &operation {
        Operation::Refresh => state.refresh(&root).map(Some),
        Operation::Choose(file, side) => state.resolve(&root, file, *side).map(Some),
        Operation::Undo(file) => state.undo(&root, file).map(Some),
        Operation::Finish(abort) => state.finish(&root, *abort).map(|()| None),
      };
      (operation, result)
    });
    self.task = Some(cx.spawn_in(window, async move |this, cx| {
      let (operation, result) = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.task = None;
        match result {
          Ok(Some(state)) => {
            if let Operation::Choose(file, side) = operation {
              this.paths.insert(file, Some(side));
            }
            this.set_state(state, cx);
          }
          Ok(None) => {
            let aborted = matches!(operation, Operation::Finish(true));
            cx.emit(if aborted { MergeEvent::Aborted } else { MergeEvent::Completed });
          }
          Err(message) => window.push_notification(Notification::error(message), cx),
        }
        cx.notify();
      });
    }));
    cx.notify();
  }

  fn confirm_abort(&self, window: &mut Window, cx: &mut Context<Self>) {
    let dialog = cx.entity().downgrade();
    window.open_alert_dialog(cx, move |alert, _, _| {
      let dialog = dialog.clone();
      alert
        .title("Abort this merge?")
        .description("The repository will return to its state before the merge. Conflict resolutions from this merge will be discarded.")
        .confirm()
        .ok_text("Abort merge")
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, window, cx| {
          let _ = dialog.update(cx, |this, cx| this.run(Operation::Finish(true), window, cx));
          true
        })
    });
  }

  fn file_row(&self, path: &str, chosen: Option<MergeSide>, cx: &mut Context<Self>) -> impl IntoElement {
    let file = self.state.files.iter().find(|file| file.path == path);
    let busy = self.task.is_some();
    let palette = colors(cx);
    let (detail, color) = if let Some(file) = file {
      (format!("{} conflict{}", file.conflicts, if file.conflicts == 1 { "" } else { "s" }), palette.conflicted)
    } else {
      let detail = match chosen {
        Some(MergeSide::Current) => format!("Using {}", self.state.current_branch),
        Some(MergeSide::Incoming) => format!("Using {}", self.source),
        None => "Resolved".into(),
      };
      (detail, palette.new)
    };
    let name: SharedString = path.to_owned().into();
    let menu_path = name.clone();
    let editor_path = name.clone();
    let undo_path = path.to_string();
    let workspace = self.root.clone();
    let view = cx.entity().downgrade();
    let sides = file.filter(|file| file.can_choose_side).map(|file| (file.ours, file.theirs));
    let branches = (self.state.current_branch.clone(), self.source.clone());
    h_flex()
      .id(SharedString::from(format!("merge-file-{name}")))
      .gap_3()
      .py_2()
      .border_b_1()
      .border_color(palette.border)
      .child(Icon::empty().path("icons/file-exclamation-point.svg").size_4().flex_none())
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .gap_1()
          .child(div().truncate().text_sm().child(name.clone()))
          .child(Label::new(detail).text_xs().text_color(color)),
      )
      .when(file.is_some(), |row| {
        row
          .child(Button::new(SharedString::from(format!("edit-merge-{name}"))).small().label("Open in editor").disabled(busy).on_click(
            move |_, window, cx| {
              window.close_dialog(cx);
              window.defer(cx, {
                let path = editor_path.clone();
                move |window, cx| window.dispatch_action(Box::new(OpenChangedFile(path)), cx)
              });
            },
          ))
          .child(
            Button::new(SharedString::from(format!("merge-options-{name}")))
              .small()
              .disabled(busy)
              .icon(IconName::ChevronDown)
              .accessibility_label("Conflict options")
              .dropdown_menu(move |menu, _, _| {
                let open_root = workspace.clone();
                let reveal_root = workspace.clone();
                let open_file = menu_path.clone();
                let reveal_file = menu_path.clone();
                let menu = menu
                  .item(PopupMenuItem::new("Open with default app").on_click(move |_, window, cx| {
                    if let Some(path) = vekro_git::workdir_file(&open_root, Path::new(open_file.as_str())).filter(|path| path.exists()) {
                      cx.open_with_system(&path);
                    } else {
                      window.push_notification(Notification::error("This file is missing from the working tree."), cx);
                    }
                  }))
                  .item(PopupMenuItem::new("Reveal in Finder").on_click(move |_, _, cx| {
                    if let Some(path) = vekro_git::workdir_file(&reveal_root, Path::new(reveal_file.as_str())) {
                      cx.reveal_path(&path);
                    }
                  }));
                if let Some((ours, theirs)) = sides {
                  [(MergeSide::Current, ours, branches.0.as_str()), (MergeSide::Incoming, theirs, branches.1.as_str())].into_iter().fold(
                    menu.separator(),
                    |menu, (side, exists, branch)| {
                      let view = view.clone();
                      let name = menu_path.to_string();
                      let label = if exists { format!("Use file from {branch}") } else { format!("Keep deletion from {branch}") };
                      menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        let _ = view.update(cx, |this, cx| this.run(Operation::Choose(name.clone(), side), window, cx));
                      }))
                    },
                  )
                } else {
                  menu
                }
              }),
          )
      })
      .when(file.is_none(), |row| {
        row
          .when(chosen.is_some(), |row| {
            row.child(
              Button::new(SharedString::from(format!("undo-merge-{name}")))
                .small()
                .label("Undo")
                .disabled(busy)
                .on_click(cx.listener(move |this, _, window, cx| this.run(Operation::Undo(undo_path.clone()), window, cx))),
            )
          })
          .child(Icon::new(IconName::Check).size_4().text_color(palette.new))
      })
  }
}

impl Render for MergeDialog {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let remaining = self.state.files.len();
    let busy = self.task.is_some();
    let palette = colors(cx);
    let paths = self.paths.clone();
    v_flex()
      .gap_3()
      .pt_2()
      .when(remaining == 0, |body| {
        body.child(
          h_flex()
            .gap_2()
            .p_3()
            .rounded_md()
            .bg(palette.diff_added)
            .child(Icon::new(IconName::Check).size_4().text_color(palette.new))
            .child(Label::new("All conflicts are resolved.")),
        )
      })
      .child(
        h_flex()
          .justify_between()
          .child(Label::new(format!("{remaining} conflicted file{}", if remaining == 1 { "" } else { "s" })).font_weight(FontWeight::SEMIBOLD))
          .child(
            h_flex()
              .gap_2()
              .child(
                Button::new("refresh-merge")
                  .ghost()
                  .small()
                  .label("Refresh")
                  .disabled(busy)
                  .on_click(cx.listener(|this, _, window, cx| this.refresh(window, cx))),
              )
              .child(Button::new("close-merge").ghost().small().label("Close").disabled(busy).on_click(|_, window, cx| window.close_dialog(cx))),
          ),
      )
      .child(
        v_flex().id("merge-files").max_h(rems(18.)).overflow_y_scrollbar().children(paths.iter().map(|(path, side)| self.file_row(path, *side, cx))),
      )
      .child(
        Label::new("When resolving manually, stage your changes in Git, then refresh this list.").text_sm().text_color(cx.theme().muted_foreground),
      )
      .child(
        h_flex()
          .gap_2()
          .justify_between()
          .pt_2()
          .child(Button::new("merge-terminal").ghost().small().label("Open terminal").disabled(busy).on_click(|_, window, cx| {
            window.close_dialog(cx);
            window.defer(cx, |window, cx| window.dispatch_action(Box::new(ToggleTerminal), cx));
          }))
          .child(
            h_flex()
              .gap_2()
              .child(
                Button::new("abort-merge")
                  .label("Abort merge")
                  .disabled(busy)
                  .on_click(cx.listener(|this, _, window, cx| this.confirm_abort(window, cx))),
              )
              .child(
                Button::new("continue-merge")
                  .primary()
                  .label("Continue merge")
                  .disabled(busy || remaining != 0)
                  .on_click(cx.listener(|this, _, window, cx| this.run(Operation::Finish(false), window, cx))),
              ),
          ),
      )
  }
}
