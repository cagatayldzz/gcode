//! The file-by-file controls for a pending Git merge.

use crate::{theme::colors, ToggleTerminal};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    label::Label,
    menu::{DropdownMenu as _, PopupMenuItem},
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
  },
  *,
};
use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
};
use ws_git::{MergeSide, MergeState};

pub(crate) enum MergeEvent {
  Completed,
  Aborted,
  /// Close the dialog and open a conflicted file (relative to the repository) in the editor. The merge stays in progress.
  OpenFile(SharedString),
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
  /// Files resolved by taking the side that has no such file, so the file is deleted.
  deleted: std::collections::HashSet<String>,
  task: Option<Task<()>>,
}

impl EventEmitter<MergeEvent> for MergeDialog {}

impl MergeDialog {
  pub fn new(root: PathBuf, source: SharedString, state: MergeState) -> Self {
    let paths = state.files.iter().map(|file| (file.path.clone(), None)).collect();
    Self { root, source, state, paths, deleted: Default::default(), task: None }
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
              let kept =
                this
                  .state
                  .files
                  .iter()
                  .find(|found| found.path == file)
                  .map(|found| if side == MergeSide::Current { found.ours } else { found.theirs });
              if kept == Some(false) {
                this.deleted.insert(file.clone());
              } else {
                this.deleted.remove(&file);
              }
              this.paths.insert(file, Some(side));
            }
            this.set_state(state, cx);
          }
          Ok(None) => {
            let aborted = matches!(operation, Operation::Finish(true));
            cx.emit(if aborted { MergeEvent::Aborted } else { MergeEvent::Completed });
          }
          Err(message) => window.push_notification(crate::failure(message), cx),
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
    let (directory, file_name) = match path.rsplit_once('/') {
      Some((directory, file_name)) => (Some(SharedString::from(format!("{directory}/"))), SharedString::from(file_name.to_owned())),
      None => (None, name.clone()),
    };
    let muted = cx.theme().muted_foreground;
    // A file one branch deleted and the other changed: nothing to merge, only a choice, made in a menu.
    let one_sided = sides.filter(|(ours, theirs)| ours != theirs).map(|(ours, _)| {
      let (current, incoming) = (branches.0.to_string(), branches.1.to_string());
      if ours {
        (MergeSide::Current, current, MergeSide::Incoming, incoming)
      } else {
        (MergeSide::Incoming, incoming, MergeSide::Current, current)
      }
    });
    let resolve = one_sided.clone().map(|(keep_side, keep_branch, drop_side, drop_branch)| {
      let (keep_view, drop_view) = (view.clone(), view.clone());
      let (keep_path, drop_path) = (menu_path.to_string(), menu_path.to_string());
      Button::new(SharedString::from(format!("merge-resolve-{name}")))
        .small()
        .outline()
        .label("Resolve")
        .dropdown_caret(true)
        .disabled(busy)
        .dropdown_menu(move |menu, _, _| {
          let (keep_view, keep_path) = (keep_view.clone(), keep_path.clone());
          let (drop_view, drop_path) = (drop_view.clone(), drop_path.clone());
          menu
            .item(PopupMenuItem::new(format!("Use the modified file from {keep_branch}")).on_click(move |_, window, cx| {
              let _ = keep_view.update(cx, |this, cx| this.run(Operation::Choose(keep_path.clone(), keep_side), window, cx));
            }))
            .item(PopupMenuItem::new(format!("Do not include this file, as {drop_branch} did")).on_click(move |_, window, cx| {
              let _ = drop_view.update(cx, |this, cx| this.run(Operation::Choose(drop_path.clone(), drop_side), window, cx));
            }))
        })
    });
    let note = match (&one_sided, chosen) {
      (Some((_, _, _, drop_branch)), _) if file.is_some() => Some((format!("File does not exist on {drop_branch}."), palette.conflicted)),
      (_, Some(side)) if file.is_none() => {
        let branch = if side == MergeSide::Current { self.state.current_branch.clone() } else { self.source.to_string() };
        let what = if self.deleted.contains(path) { "the deleted file" } else { "the file" };
        Some((format!("Using {what} from {branch}"), palette.new))
      }
      _ => None,
    };
    let flat = file.is_none() || resolve.is_some();
    let status = if flat {
      Icon::new(IconName::File).size_4().flex_none().text_color(muted)
    } else if file.is_some() {
      Icon::empty().path("icons/triangle-alert.svg").size_4().flex_none().text_color(palette.conflicted)
    } else {
      Icon::new(IconName::Check).size_4().flex_none().text_color(palette.new)
    };
    // The buttons of a conflicted file: take one side whole, or edit it by hand.
    let actions = file.filter(|_| resolve.is_none()).map(|_| {
      let choose = sides.map(|(ours, theirs)| {
        [(MergeSide::Current, ours, branches.0.clone()), (MergeSide::Incoming, theirs, branches.1.to_string())].map(|(side, exists, branch)| {
          let view = view.clone();
          let choice = menu_path.to_string();
          let (label, tip) = if exists {
            (format!("Use {branch}"), format!("Resolve the whole file with the version from {branch}"))
          } else {
            (format!("Delete as in {branch}"), format!("Resolve by deleting the file, as {branch} did"))
          };
          Button::new(SharedString::from(format!("merge-use-{side:?}-{name}"))).small().outline().label(label).tooltip(tip).disabled(busy).on_click(
            move |_, window, cx| {
              let _ = view.update(cx, |this, cx| this.run(Operation::Choose(choice.clone(), side), window, cx));
            },
          )
        })
      });
      h_flex()
        .gap_2()
        .flex_wrap()
        .children(choose.into_iter().flatten())
        .child(
          Button::new(SharedString::from(format!("edit-merge-{name}")))
            .small()
            .primary()
            .icon(Icon::empty().path("icons/pencil.svg"))
            .label("Resolve in editor")
            .disabled(busy)
            .on_click({
              let view = view.clone();
              move |_, _, cx| {
                // The workspace opens the file: an action dispatched from here would go to whatever has the focus once the dialog closes.
                let path = editor_path.clone();
                let _ = view.update(cx, |_, cx| cx.emit(MergeEvent::OpenFile(path)));
              }
            }),
        )
        .child(
          Button::new(SharedString::from(format!("merge-options-{name}")))
            .small()
            .ghost()
            .disabled(busy)
            .icon(IconName::Ellipsis)
            .accessibility_label("More options")
            .dropdown_menu(move |menu, _, _| {
              let open_root = workspace.clone();
              let reveal_root = workspace.clone();
              let open_file = menu_path.clone();
              let reveal_file = menu_path.clone();
              menu
                .item(PopupMenuItem::new("Open with default app").on_click(move |_, window, cx| {
                  if let Some(path) = ws_git::workdir_file(&open_root, Path::new(open_file.as_str())).filter(|path| path.exists()) {
                    cx.open_with_system(&path);
                  } else {
                    window.push_notification(crate::failure("This file is missing from the working tree."), cx);
                  }
                }))
                .item(PopupMenuItem::new("Reveal in Finder").on_click(move |_, _, cx| {
                  if let Some(path) = ws_git::workdir_file(&reveal_root, Path::new(reveal_file.as_str())) {
                    cx.reveal_path(&path);
                  }
                }))
            }),
        )
    });
    v_flex()
      .id(SharedString::from(format!("merge-file-{name}")))
      .gap_2()
      .when(flat, |row| row.px_1().py_2())
      .when(!flat, |row| row.p_3().rounded_md().border_1().border_color(palette.border))
      .child(
        h_flex()
          .gap_3()
          .items_center()
          .child(status)
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .child(div().truncate().text_sm().font_weight(FontWeight::MEDIUM).child(file_name))
              .children(directory.map(|directory| div().truncate().text_xs().text_color(muted).child(directory)))
              .children(note.clone().map(|(note, color)| div().text_xs().text_color(color).child(note))),
          )
          .when(resolve.is_none() && note.is_none(), |row| {
            row.child(Label::new(detail).text_xs().font_weight(FontWeight::MEDIUM).text_color(color).flex_none())
          })
          .children(resolve)
          .when(file.is_none() && chosen.is_some(), |row| {
            row.child(
              Button::new(SharedString::from(format!("undo-merge-{name}")))
                .small()
                .ghost()
                .icon(Icon::empty().path("icons/undo-2.svg"))
                .label("Undo")
                .disabled(busy)
                .on_click(cx.listener(move |this, _, window, cx| this.run(Operation::Undo(undo_path.clone()), window, cx))),
            )
          })
          .when(file.is_none(), |row| {
            row.child(
              div()
                .size_5()
                .flex_none()
                .rounded_full()
                .bg(palette.new)
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(IconName::Check).size_3().text_color(palette.on_accent)),
            )
          }),
      )
      .children(actions)
  }
}

impl Render for MergeDialog {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let remaining = self.state.files.len();
    let busy = self.task.is_some();
    let palette = colors(cx);
    let muted = cx.theme().muted_foreground;
    // The files that still need a decision come first.
    let (open, done): (Vec<_>, Vec<_>) = self.paths.clone().into_iter().partition(|(path, _)| self.state.files.iter().any(|file| &file.path == path));
    let bold = |text: SharedString| Label::new(text).font_weight(FontWeight::SEMIBOLD);
    v_flex()
      .gap_3()
      .pt_2()
      .child(
        h_flex()
          .gap_2()
          .items_center()
          .justify_between()
          .child(
            h_flex()
              .gap_1()
              .items_center()
              .min_w_0()
              .child(Icon::empty().path("icons/git-merge.svg").size_4().text_color(muted))
              .child(Label::new("Merging").text_color(muted))
              .child(bold(self.source.clone()))
              .child(Label::new("into").text_color(muted))
              .child(bold(self.state.current_branch.clone().into())),
          )
          .child(
            h_flex()
              .gap_1()
              .child(
                Button::new("refresh-merge")
                  .ghost()
                  .small()
                  .icon(Icon::empty().path("icons/refresh-cw.svg"))
                  .label("Refresh")
                  .disabled(busy)
                  .on_click(cx.listener(|this, _, window, cx| this.refresh(window, cx))),
              )
              .child(Button::new("close-merge").ghost().small().label("Close").disabled(busy).on_click(|_, window, cx| window.close_dialog(cx))),
          ),
      )
      .when(remaining == 0, |body| {
        body.child(
          h_flex()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(palette.diff_added)
            .child(Icon::new(IconName::Check).size_4().text_color(palette.new))
            .child(Label::new("All conflicted files have been resolved.")),
        )
      })
      .child(Label::new(format!("{remaining} conflicted file{}", if remaining == 1 { "" } else { "s" })).font_weight(FontWeight::SEMIBOLD))
      .child(
        v_flex()
          .id("merge-files")
          .max_h(rems(24.))
          .overflow_y_scrollbar()
          .gap_2()
          .children(open.iter().map(|(path, side)| self.file_row(path, *side, cx)))
          .children(done.iter().map(|(path, side)| self.file_row(path, *side, cx))),
      )
      .child(
        h_flex()
          .flex_wrap()
          .text_sm()
          .text_color(muted)
          .child(
            div()
              .id("merge-command-line")
              .cursor_pointer()
              .text_color(palette.link)
              .hover(|link| link.text_color(palette.link_hover).underline())
              .child("Open in command line")
              .on_click(|_, window, cx| {
                window.close_dialog(cx);
                window.defer(cx, |window, cx| window.dispatch_action(Box::new(ToggleTerminal), cx));
              }),
          )
          .child(", your tool of choice, or close to resolve manually."),
      )
      .child(
        h_flex()
          .gap_2()
          .justify_end()
          .items_center()
          .pt_3()
          .border_t_1()
          .border_color(palette.border)
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
              .loading(busy)
              .disabled(busy || remaining != 0)
              .on_click(cx.listener(|this, _, window, cx| this.run(Operation::Finish(false), window, cx))),
          ),
      )
  }
}
