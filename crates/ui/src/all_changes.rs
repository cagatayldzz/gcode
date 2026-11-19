//! The working tree's changes in one virtualized Kit diff. Each file's header carries the review controls.

use crate::{
  diff_view::ShownDiff,
  settings::code_font_size,
  theme::{colors, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    clipboard::Clipboard,
    diff::{DiffFile, DiffState},
    h_flex,
    label::Label,
    menu::{DropdownMenu as _, PopupMenuItem},
    progress::ProgressCircle,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, StyledExt as _,
  },
  *,
};
use std::{collections::HashSet, path::PathBuf, sync::Arc};

#[derive(Action, Clone, PartialEq)]
#[action(namespace = vekro, no_json)]
pub(crate) struct OpenChangedFile(pub SharedString);

#[derive(Default)]
pub(crate) struct AllChanges {
  diff: Option<ShownDiff>,
  patch: Option<Arc<str>>,
  viewed: HashSet<SharedString>,
  error: Option<SharedString>,
  task: Option<Task<()>>,
}

impl AllChanges {
  pub fn reset(&mut self, cx: &mut Context<Self>) {
    *self = Self::default();
    cx.notify();
  }

  pub fn reload(&mut self, root: PathBuf, cx: &mut Context<Self>) {
    if self.task.is_some() {
      return;
    }
    let previous = self.patch.clone();
    let work = cx.background_executor().spawn(async move {
      let patch = vekro_git::all_changes_diff(&root)?;
      if previous.as_deref() == Some(patch.as_str()) {
        return Ok(None);
      }
      let files = DiffFile::parse(&patch).map_err(|error| error.to_string())?;
      Ok::<_, String>(Some((patch, files)))
    });
    self.task = Some(cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        this.task = None;
        match result {
          Ok(None) => return,
          Ok(Some((patch, files))) => {
            let paths: HashSet<_> = files.iter().map(DiffFile::path).collect();
            this.viewed.retain(|path| paths.contains(path));
            if let Some(diff) = &mut this.diff {
              diff.set_files(files, &patch, cx);
            } else {
              this.diff = Some(ShownDiff::new(files, &patch, cx));
            }
            this.patch = Some(patch.into());
            this.error = None;
          }
          Err(error) => {
            this.patch = None;
            this.error = Some(format!("Could not read all changes: {error}").into());
          }
        }
        cx.notify();
      });
    }));
  }
}

impl AllChanges {
  /// Scrolls to a file's header and opens it, whether it was folded or not.
  pub fn scroll_to(&mut self, path: &str, cx: &mut Context<Self>) {
    if let Some(diff) = &self.diff {
      diff.state().update(cx, |state, cx| {
        state.set_file_collapsed(path, false, cx);
        state.scroll_to_file(path, cx);
      });
    }
  }

  /// Jumps to the first file not marked as viewed.
  fn next_unviewed(&mut self, cx: &mut Context<Self>) {
    let Some(diff) = &self.diff else { return };
    let next = diff.state().read(cx).files().iter().map(|file| file.path().clone()).find(|path| !self.viewed.contains(path));
    if let Some(path) = next {
      self.scroll_to(&path, cx);
    }
  }

  /// Folds every file to its header, or opens them all.
  fn set_all_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
    if let Some(diff) = &self.diff {
      diff.state().update(cx, |state, cx| state.set_all_files_collapsed(collapsed, cx));
    }
  }

  /// The bar above the diff: what changed in total, the jump menu, the review shortcuts and the progress.
  fn toolbar(&self, state: &Entity<DiffState>, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let muted = cx.theme().muted_foreground;
    let files: Vec<(SharedString, usize, usize)> =
      state.read(cx).files().iter().map(|file| (file.path().clone(), file.additions(), file.deletions())).collect();
    let (count, viewed) = (files.len(), self.viewed.len());
    let (added, removed) = files.iter().fold((0, 0), |(added, removed), (_, a, d)| (added + a, removed + d));
    let all_viewed = viewed >= count;
    let reviewed = self.viewed.clone();
    let menu_state = state.clone();
    let jump = Button::new("all-changes-files")
      .ghost()
      .small()
      .icon(Icon::empty().path("icons/files.svg"))
      .label("Jump to file")
      .child(Icon::new(IconName::ChevronDown).size_3())
      .dropdown_menu(move |menu, _, _| {
        files.iter().fold(menu.scrollable(true).max_h(px(360.)), |menu, (path, added, removed)| {
          let (target, state) = (path.clone(), menu_state.clone());
          menu.item(PopupMenuItem::new(format!("{path}   +{added} −{removed}")).checked(reviewed.contains(path)).on_click(move |_, _, cx| {
            state.update(cx, |state, cx| {
              state.set_file_collapsed(&target, false, cx);
              state.scroll_to_file(&target, cx);
            })
          }))
        })
      });
    h_flex()
      .flex_none()
      .gap_3()
      .px_4()
      .py_1p5()
      .items_center()
      .border_b_1()
      .border_color(palette.border)
      .bg(palette.surface_alt)
      .child(jump)
      .child(
        h_flex()
          .gap_2()
          .items_center()
          .text_size(FONT_SIZE_SM)
          .child(Label::new(if count == 1 { "1 file".to_string() } else { format!("{count} files") }).text_color(muted))
          .child(div().text_color(palette.new).child(format!("+{added}")))
          .child(div().text_color(palette.deleted).child(format!("−{removed}"))),
      )
      .child(div().flex_1())
      .child(
        Button::new("all-changes-next")
          .ghost()
          .small()
          .icon(Icon::empty().path("icons/arrow-down-to-line.svg"))
          .label("Next unviewed")
          .disabled(all_viewed)
          .on_click(cx.listener(|this, _, _, cx| this.next_unviewed(cx))),
      )
      .child(
        Button::new("all-changes-collapse")
          .ghost()
          .small()
          .icon(Icon::empty().path("icons/chevrons-down-up.svg"))
          .tooltip("Collapse all files")
          .accessibility_label("Collapse all files")
          .on_click(cx.listener(|this, _, _, cx| this.set_all_collapsed(true, cx))),
      )
      .child(
        Button::new("all-changes-expand")
          .ghost()
          .small()
          .icon(Icon::empty().path("icons/chevrons-up-down.svg"))
          .tooltip("Expand all files")
          .accessibility_label("Expand all files")
          .on_click(cx.listener(|this, _, _, cx| this.set_all_collapsed(false, cx))),
      )
      .child(
        h_flex()
          .gap_1p5()
          .items_center()
          .text_size(FONT_SIZE_SM)
          .text_color(if all_viewed { palette.new } else { muted })
          .child(ProgressCircle::new("viewed-progress").xsmall().value(100. * viewed as f32 / count.max(1) as f32))
          .child(format!("{viewed} / {count} viewed")),
      )
  }
}

impl Render for AllChanges {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    if let Some(error) = &self.error {
      return Label::new(error.clone()).p_4().text_color(cx.theme().danger).into_any_element();
    }
    if self.patch.as_deref() == Some("") {
      return Label::new("No changes to preview.").p_4().text_color(muted).into_any_element();
    }
    let Some(diff) = &self.diff else {
      return h_flex().gap_2().p_4().child(Spinner::new().small()).child(Label::new("Loading changes…").text_color(muted)).into_any_element();
    };
    let state = diff.state().clone();
    let view = cx.entity().downgrade();
    let diff = diff
      .view(cx)
      .soft_wrap(true)
      .text_size(code_font_size(cx))
      .border_0()
      .size_full()
      .render_header(move |file, _, cx| file_header(file, &view, cx));
    v_flex()
      .size_full()
      .min_h_0()
      .child(self.toolbar(&state, cx))
      // The diff owns its scrolling, so its scrollbar sits at the right edge of the area.
      .child(div().flex_1().min_h_0().overflow_hidden().cursor_text().child(diff))
      .into_any_element()
  }
}

/// The content of a file's header: its path with the open and copy buttons, the size of the change, and the review checkbox.
fn file_header(file: &DiffFile, view: &WeakEntity<AllChanges>, cx: &App) -> impl IntoElement {
  let palette = colors(cx);
  let (added, removed) = (file.additions(), file.deletions());
  let total = added + removed;
  let blocks = total.min(5);
  let added_blocks = match (added, removed) {
    (0, _) => 0,
    (_, 0) => blocks,
    _ => ((added as f64 / total as f64 * blocks as f64).round() as usize).clamp(1, blocks - 1),
  };
  let path = file.path().clone();
  let checked = view.upgrade().is_some_and(|view| view.read(cx).viewed.contains(&path));
  let modified = file.modified_path().cloned();
  let title = match (file.original_path(), file.modified_path()) {
    (Some(old), Some(new)) if old != new => SharedString::from(format!("{old} → {new}")),
    _ => path.clone(),
  };
  let view = view.clone();
  h_flex()
    .w_full()
    .min_w_0()
    .gap_3()
    .child(
      h_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .child(file_title(&path, &title, cx))
        .when_some(modified, |row, modified| {
          row.child(
            div().cursor_pointer().child(
              Button::new(SharedString::from(format!("open-diff-file-{path}")))
                .ghost()
                .xsmall()
                .icon(Icon::empty().path("icons/code-xml.svg"))
                .tooltip("Open in Explorer")
                .accessibility_label("Open in Explorer")
                .on_click(move |_, window, cx| window.dispatch_action(Box::new(OpenChangedFile(modified.clone())), cx)),
            ),
          )
        })
        .child(
          div()
            .cursor_pointer()
            .child(Clipboard::new(SharedString::from(format!("copy-diff-path-{path}"))).value(path.clone()).tooltip("Copy relative path")),
        ),
    )
    .when(total > 0, |row| {
      row.child(
        h_flex()
          .flex_none()
          .gap_1()
          .child(div().text_xs().text_color(palette.new).child(format!("+{added}")))
          .child(div().text_xs().text_color(palette.deleted).child(format!("−{removed}")))
          .child(h_flex().gap_0p5().children((0..5).map(|index| {
            let color = if index < added_blocks {
              palette.new
            } else if index < blocks {
              palette.deleted
            } else {
              palette.border
            };
            div().size_1p5().rounded_sm().bg(color)
          }))),
      )
    })
    // The diff area shows a text cursor; the controls in the header are clickable.
    .child(div().cursor_pointer().child(Checkbox::new(SharedString::from(format!("diff-viewed-{path}"))).label("Viewed").checked(checked).on_click(
      move |checked, _, cx| {
        let _ = view.update(cx, |this, cx| {
          if *checked {
            this.viewed.insert(path.clone());
          } else {
            this.viewed.remove(&path);
          }
          // A file that has been reviewed folds away; unchecking opens it again.
          if let Some(diff) = &this.diff {
            diff.state().update(cx, |state, cx| state.set_file_collapsed(&path, *checked, cx));
          }
          cx.notify();
        });
      },
    )))
}

/// The file's name in bold with its folder after it, soft: the name is what the eye looks for. A rename shows both paths.
fn file_title(path: &str, title: &str, cx: &App) -> impl IntoElement {
  let palette = colors(cx);
  let renamed = title != path;
  let (folder, name) = match path.rsplit_once('/') {
    Some((folder, name)) if !renamed => (Some(format!("{folder}/")), name),
    _ => (None, title),
  };
  h_flex()
    .min_w_0()
    .overflow_hidden()
    .whitespace_nowrap()
    .gap_0()
    .text_sm()
    .child(div().flex_none().font_semibold().child(name.to_string()))
    .children(folder.map(|folder| {
      div().ml_2().min_w_0().overflow_hidden().text_ellipsis().text_size(FONT_SIZE_SM).text_color(palette.text_secondary).child(folder)
    }))
}
