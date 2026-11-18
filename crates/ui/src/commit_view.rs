//! A commit of the History tab shown in the main area: its message, the files it changed and the diff of the chosen file.

use crate::{
  diff_view::ShownDiff,
  settings::code_font_size,
  theme::{colors, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    clipboard::Clipboard, diff::DiffFile, h_flex, label::Label, scroll::ScrollableElement as _, spinner::Spinner, v_flex, ActiveTheme as _, Icon,
    Sizable as _,
  },
  *,
};
use std::path::PathBuf;
use vekro_git::{ChangeKind, CommitDetail};

/// The width of the list of changed files.
const FILES_WIDTH: Pixels = px(280.);

pub(crate) enum Load<T> {
  Loading,
  Ready(T),
  Failed(SharedString),
}

pub(crate) struct CommitView {
  root: PathBuf,
  id: String,
  detail: Load<CommitDetail>,
  selected: Option<PathBuf>,
  diff: Load<ShownDiff>,
  /// Which commit and file the answers being awaited belong to, so a late one for another is ignored.
  epoch: u64,
}

impl CommitView {
  pub fn new() -> Self {
    Self { root: PathBuf::new(), id: String::new(), detail: Load::Loading, selected: None, diff: Load::Loading, epoch: 0 }
  }

  /// Shows another commit of the repository at `root`, the first of its files selected.
  pub fn show(&mut self, root: PathBuf, id: String, cx: &mut Context<Self>) {
    if self.root == root && self.id == id {
      return;
    }
    self.root = root.clone();
    self.id = id.clone();
    self.detail = Load::Loading;
    self.selected = None;
    self.diff = Load::Loading;
    self.epoch += 1;
    let epoch = self.epoch;
    cx.notify();
    let work = cx.background_executor().spawn(async move { vekro_git::commit_detail(&root, &id) });
    cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.epoch != epoch {
          return;
        }
        match result {
          Ok(detail) => {
            let first = detail.files.first().map(|change| change.path.clone());
            this.detail = Load::Ready(detail);
            match first {
              Some(file) => this.select_file(file, cx),
              None => this.diff = Load::Failed("This commit changed no files.".into()),
            }
          }
          Err(message) => this.detail = Load::Failed(message.into()),
        }
        cx.notify();
      });
    })
    .detach();
  }

  fn select_file(&mut self, file: PathBuf, cx: &mut Context<Self>) {
    self.selected = Some(file.clone());
    self.diff = Load::Loading;
    self.epoch += 1;
    let (epoch, root, id) = (self.epoch, self.root.clone(), self.id.clone());
    cx.notify();
    let work = cx.background_executor().spawn(async move { vekro_git::commit_file_diff(&root, &id, &file) });
    cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.epoch != epoch {
          return;
        }
        this.diff = match result.and_then(|patch| DiffFile::parse(&patch).map(|files| (files, patch)).map_err(|error| error.to_string())) {
          Ok((files, _)) if files.is_empty() => Load::Failed("This file has no text changes to show.".into()),
          Ok((files, patch)) => Load::Ready(ShownDiff::new(files, &patch, cx)),
          Err(message) => Load::Failed(message.into()),
        };
        cx.notify();
      });
    })
    .detach();
  }

  fn file_row(&self, change: &vekro_git::Change, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let theme = cx.theme().clone();
    let file = change.path.clone();
    let selected = self.selected.as_deref() == Some(file.as_path());
    let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let folder = file.parent().map(|parent| parent.display().to_string()).filter(|parent| !parent.is_empty());
    let mark = match change.kind {
      ChangeKind::New => palette.new,
      ChangeKind::Deleted => palette.deleted,
      ChangeKind::Renamed => palette.renamed,
      ChangeKind::Conflicted => palette.conflicted,
      ChangeKind::Modified => palette.modified,
    };
    h_flex()
      .id(SharedString::from(format!("commit-file-{}", file.display())))
      .gap_1p5()
      .px_3()
      .py_1()
      .items_center()
      .cursor_pointer()
      .when(selected, |row| row.bg(palette.selected))
      .hover(|row| row.bg(palette.hover))
      .child(Label::new(change.kind.mark()).w_4().flex_none().text_color(mark))
      .child(Label::new(name).min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis())
      .when_some(folder, |row, folder| {
        row.child(
          Label::new(folder)
            .text_size(FONT_SIZE_SM)
            .text_color(theme.muted_foreground)
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis(),
        )
      })
      .on_click(cx.listener(move |this, _, _, cx| this.select_file(file.clone(), cx)))
  }
}

impl Render for CommitView {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let message = |text: SharedString, danger: bool| {
      Label::new(text).p_4().text_color(if danger { theme.danger } else { theme.muted_foreground }).into_any_element()
    };
    let spinner = || {
      h_flex()
        .gap_2()
        .p_4()
        .items_center()
        .child(Spinner::new().small())
        .child(Label::new("Loading…").text_color(theme.muted_foreground))
        .into_any_element()
    };
    let detail = match &self.detail {
      Load::Loading => return v_flex().size_full().child(spinner()),
      Load::Failed(error) => return v_flex().size_full().child(message(error.clone(), true)),
      Load::Ready(detail) => detail,
    };
    let id = detail.info.id.clone();
    let header = v_flex()
      .flex_none()
      .gap_1()
      .px_4()
      .py_2p5()
      .border_b_1()
      .border_color(palette.border)
      .child(
        h_flex()
          .gap_3()
          .items_center()
          .child(
            Label::new(detail.info.summary.clone())
              .font_weight(FontWeight::SEMIBOLD)
              .flex_1()
              .min_w_0()
              .overflow_hidden()
              .whitespace_nowrap()
              .text_ellipsis(),
          )
          .child(Label::new(format!("+{}", detail.additions)).text_size(FONT_SIZE_SM).text_color(palette.new))
          .child(Label::new(format!("-{}", detail.deletions)).text_size(FONT_SIZE_SM).text_color(palette.deleted)),
      )
      .child(
        h_flex()
          .gap_2()
          .items_center()
          .child(Label::new(detail.info.author.clone()).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
          .child(Icon::empty().path("icons/git-commit-horizontal.svg").size_4().text_color(theme.muted_foreground))
          .child(Label::new(detail.info.short.clone()).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
          .child(
            Clipboard::new(SharedString::from(format!("copy-commit-id-{id}")))
              .value(id)
              .accessibility_label("Copy the full commit id")
              .tooltip("Copy the full commit id"),
          ),
      )
      .when(!detail.body.is_empty(), |column| {
        column.child(Label::new(detail.body.clone()).text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
      });
    let count = detail.files.len();
    let files = v_flex()
      .w(FILES_WIDTH)
      .flex_none()
      .h_full()
      .border_r_1()
      .border_color(palette.border)
      .child(
        Label::new(if count == 1 { "1 changed file".to_string() } else { format!("{count} changed files") })
          .px_3()
          .py_1p5()
          .border_b_1()
          .border_color(palette.border),
      )
      .child(
        v_flex().id("commit-files").flex_1().min_h_0().overflow_y_scrollbar().children(detail.files.iter().map(|change| self.file_row(change, cx))),
      );
    let diff = match &self.diff {
      Load::Loading => spinner(),
      Load::Failed(error) => message(error.clone(), false),
      Load::Ready(state) => state.view(cx).soft_wrap(true).border_0().text_size(code_font_size(cx)).size_full().into_any_element(),
    };
    v_flex()
      .size_full()
      .bg(theme.background)
      .child(header)
      .child(h_flex().flex_1().min_h_0().child(files).child(div().flex_1().min_w_0().h_full().child(diff)))
  }
}
