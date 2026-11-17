//! The changes of one file of the Git tab, shown in the main area in place of the editor, like GitHub Desktop: the path on top,
//! the diff below. Choosing another file replaces it; no tab is added.

use crate::{
  commit_view::Load,
  settings::code_font_size,
  theme::{colors, FONT_SIZE_SM},
};
use gpui_kit::{
  component::{
    diff::{Diff, DiffFile, DiffMode, DiffState},
    h_flex,
    label::Label,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Sizable as _,
  },
  *,
};
use std::path::PathBuf;

pub(crate) struct ChangeView {
  root: PathBuf,
  file: PathBuf,
  diff: Load<Entity<DiffState>>,
  /// The patch the shown diff was made from, to leave the diff (and its scroll position) alone while nothing changed.
  patch: String,
  /// Which read the answer being awaited belongs to.
  epoch: u64,
}

impl ChangeView {
  pub fn new() -> Self {
    Self { root: PathBuf::new(), file: PathBuf::new(), diff: Load::Loading, patch: String::new(), epoch: 0 }
  }

  /// Shows the changes of another file of the repository at `root`.
  pub fn show(&mut self, root: PathBuf, file: PathBuf, cx: &mut Context<Self>) {
    if self.root == root && self.file == file {
      return;
    }
    self.root = root;
    self.file = file;
    self.diff = Load::Loading;
    self.patch.clear();
    self.reload(cx);
  }

  /// Reads the file's changes again, and redraws only if they differ from what is shown.
  pub fn reload(&mut self, cx: &mut Context<Self>) {
    self.epoch += 1;
    let (epoch, root, file) = (self.epoch, self.root.clone(), self.file.clone());
    cx.notify();
    let work = cx.background_executor().spawn(async move { vekro_git::file_diff(&root, &file) });
    cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.epoch != epoch || (matches!(this.diff, Load::Ready(_)) && result.as_deref() == Ok(this.patch.as_str())) {
          return;
        }
        match result {
          Ok(patch) => {
            this.diff = match DiffFile::parse(&patch) {
              Ok(files) if files.is_empty() => Load::Failed("This file has no text changes to show.".into()),
              Ok(files) => Load::Ready(cx.new(|cx| DiffState::new(files, cx).with_mode(DiffMode::Unified))),
              Err(error) => Load::Failed(format!("Could not read the changes: {error}").into()),
            };
            this.patch = patch;
          }
          Err(message) => this.diff = Load::Failed(message.into()),
        }
        cx.notify();
      });
    })
    .detach();
  }
}

impl Render for ChangeView {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let name = self.file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let folder = self.file.parent().map(|parent| parent.display().to_string()).filter(|parent| !parent.is_empty());
    let header = h_flex()
      .flex_none()
      .px_4()
      .py_1p5()
      .border_b_1()
      .border_color(palette.border)
      .child(match folder {
        Some(folder) => Label::new(format!("{folder}/")).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground).into_any_element(),
        None => div().into_any_element(),
      })
      .child(Label::new(name).text_size(FONT_SIZE_SM).font_weight(FontWeight::SEMIBOLD));
    let body = match &self.diff {
      Load::Loading => h_flex()
        .gap_2()
        .p_4()
        .items_center()
        .child(Spinner::new().small())
        .child(Label::new("Loading…").text_color(theme.muted_foreground))
        .into_any_element(),
      Load::Failed(message) => Label::new(message.clone()).p_4().text_color(theme.muted_foreground).into_any_element(),
      Load::Ready(state) => Diff::new(state).soft_wrap(true).border_0().text_size(code_font_size(cx)).size_full().into_any_element(),
    };
    v_flex().size_full().bg(theme.background).child(header).child(div().flex_1().min_h_0().child(body))
  }
}
