//! The working tree's changes in one virtualized diff with per-file review controls.

use crate::{diff_view::ShownDiff, settings::code_font_size, theme::colors};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{checkbox::Checkbox, clipboard::Clipboard, diff::DiffFile, h_flex, label::Label, spinner::Spinner, ActiveTheme as _, Sizable as _},
  *,
};
use std::{collections::HashSet, path::PathBuf, sync::Arc};

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
    let view = cx.entity().downgrade();
    let viewed = self.viewed.clone();
    diff
      .view(cx)
      .soft_wrap(true)
      .text_size(code_font_size(cx))
      .border_0()
      .size_full()
      .render_header(move |file, _, cx| {
        let palette = colors(cx);
        let added = file.additions();
        let removed = file.deletions();
        let total = added + removed;
        let blocks = total.min(5);
        let added_blocks = match (added, removed) {
          (0, _) => 0,
          (_, 0) => blocks,
          _ => ((added as f64 / total as f64 * blocks as f64).round() as usize).clamp(1, blocks - 1),
        };
        let path = file.path().clone();
        let checked = viewed.contains(&path);
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
              .child(div().text_sm().min_w_0().truncate().child(title))
              .child(Clipboard::new(SharedString::from(format!("copy-diff-path-{path}"))).value(path.clone()).tooltip("Copy relative path")),
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
          .child(Checkbox::new(SharedString::from(format!("diff-viewed-{path}"))).label("Viewed").checked(checked).on_click(move |checked, _, cx| {
            let _ = view.update(cx, |this, cx| {
              if *checked {
                this.viewed.insert(path.clone());
              } else {
                this.viewed.remove(&path);
              }
              cx.notify();
            });
          }))
      })
      .into_any_element()
  }
}
