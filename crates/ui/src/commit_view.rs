//! A commit of the History tab shown in the main area: its message, the files it changed and the diff of the chosen file.

use crate::{
  diff_table::{prepare, table_controls, DiffTable},
  settings::SettingsModel,
  theme::{colors, row_divider, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    avatar::Avatar,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    label::Label,
    resizable::{h_resizable, resizable_panel},
    scroll::{Scrollbar, ScrollbarAxis, ScrollbarMode},
    spinner::Spinner,
    v_flex, v_virtual_list, ActiveTheme as _, Icon, Sizable as _, VirtualListScrollHandle,
  },
  *,
};
use std::{path::PathBuf, rc::Rc};
use ws_git::{ChangeKind, CommitDetail};

/// The fixed height of a changed file's line: the virtualized list needs every row's height before it draws.
const FILE_ROW_HEIGHT: Pixels = px(28.);

/// The text size of the list of changed files: one step below the app's text size, like the Changes list.
const FILE_FONT_SIZE: Pixels = crate::components::CHANGE_PATH_FONT_SIZE;

/// The width of the list of changed files when a commit is opened, and how narrow and how wide it can be dragged.
const FILES_WIDTH: Pixels = px(280.);
const FILES_MIN_WIDTH: Pixels = px(200.);
const FILES_MAX_WIDTH: Pixels = px(800.);

/// What a changed file's line keeps for other things than the path: the padding, the status mark and the gap before the path.
const PATH_MARGIN: f32 = 54.;

/// An estimate of the width of one character of the path, from the font size.
const CHAR_WIDTH: f32 = 0.54 * 13.;

/// The folder of a changed file with a trailing slash, as much of it as fits next to `name`: when it does not all fit, the folders in the
/// middle are replaced by `…` (`app/settings/users/…/`) and the first ones are kept, the way a path is shortened in a long file list.
fn shortened_folder(folder: &str, name: &str, width: f32) -> String {
  let room = ((width - PATH_MARGIN) / CHAR_WIDTH) as usize;
  let fits = |text: &str| text.chars().count() + name.chars().count() <= room;
  let whole = format!("{folder}/");
  if fits(&whole) {
    return whole;
  }
  let folders: Vec<&str> = folder.split('/').collect();
  (1..folders.len())
    .rev()
    .map(|kept| format!("{}/…/", folders[..kept].join("/")))
    .find(|shortened| fits(shortened))
    .unwrap_or_else(|| "…/".to_string())
}

/// The tallest the commit message grows to: a few lines when folded, most of the window when unfolded; longer messages scroll.
const MESSAGE_FOLDED_HEIGHT: Rems = rems(6.5);
const MESSAGE_UNFOLDED_HEIGHT: Rems = rems(28.);

pub(crate) enum Load<T> {
  Loading,
  Ready(T),
  Failed(SharedString),
}

pub(crate) struct CommitView {
  /// The settings of the profile, for the size of code.
  settings: Entity<SettingsModel>,
  root: PathBuf,
  id: String,
  detail: Load<CommitDetail>,
  selected: Option<PathBuf>,
  diff: Load<Entity<DiffTable>>,
  /// Which commit and file the answers being awaited belong to, so a late one for another is ignored.
  epoch: u64,
  /// The message box is unfolded to show more of the message, and the details of the commit are written out.
  message_unfolded: bool,
  message_scroll: ScrollHandle,
  /// The scroll position and row sizes of the list of changed files, which only builds the lines in view.
  files_scroll: VirtualListScrollHandle,
  file_sizes: Rc<Vec<Size<Pixels>>>,
}

impl CommitView {
  pub fn new(settings: Entity<SettingsModel>) -> Self {
    Self {
      settings,
      root: PathBuf::new(),
      id: String::new(),
      detail: Load::Loading,
      selected: None,
      diff: Load::Loading,
      epoch: 0,
      message_unfolded: false,
      message_scroll: ScrollHandle::new(),
      files_scroll: VirtualListScrollHandle::new(),
      file_sizes: Rc::new(Vec::new()),
    }
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
    self.message_scroll.set_offset(point(px(0.), px(0.)));
    self.epoch += 1;
    let epoch = self.epoch;
    cx.notify();
    let work = cx.background_executor().spawn(async move { ws_git::commit_detail(&root, &id) });
    cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.epoch != epoch {
          return;
        }
        match result {
          Ok(detail) => {
            let first = detail.files.first().map(|change| change.path.clone());
            this.file_sizes = Rc::new(vec![size(px(0.), FILE_ROW_HEIGHT); detail.files.len()]);
            this.files_scroll.set_offset(point(px(0.), px(0.)));
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
    let theme = crate::diff_view::highlight_theme(cx);
    let work = cx.background_executor().spawn(async move {
      let patch = ws_git::commit_file_diff(&root, &id, &file)?;
      crate::diff_view::warm(&theme, &patch);
      prepare(&theme, &patch, &file)
    });
    cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.epoch != epoch {
          return;
        }
        this.diff = match result {
          Ok(prepared) => {
            let settings = this.settings.clone();
            Load::Ready(cx.new(|cx| DiffTable::new(prepared, settings, cx)))
          }
          Err(message) => Load::Failed(message.into()),
        };
        cx.notify();
      });
    })
    .detach();
  }

  fn file_row(&self, change: &ws_git::Change, width: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
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
      .when(width.is_zero(), |row| row.w_full())
      .when(!width.is_zero(), |row| row.w(width))
      .min_w_0()
      .overflow_hidden()
      .h(FILE_ROW_HEIGHT)
      .gap_1p5()
      .px_3()
      .items_center()
      .border_b_1()
      .border_color(row_divider(&palette))
      .cursor_pointer()
      .when(selected, |row| row.bg(palette.selected))
      .hover(|row| row.bg(palette.hover))
      .child(Label::new(change.kind.mark()).text_size(FILE_FONT_SIZE).w_4().flex_none().text_color(mark))
      // One path: the folder dimmed, then the file name right after it with no gap. The middle of the folder gives way when the row is narrow.
      .child(
        h_flex()
          .flex_1()
          .min_w_0()
          .overflow_hidden()
          .when_some(
            folder.map(|folder| shortened_folder(&folder, &name, if width.is_zero() { FILES_WIDTH.as_f32() } else { width.as_f32() })),
            |path, folder| {
              path.child(Label::new(folder).text_size(FILE_FONT_SIZE).text_color(theme.muted_foreground).flex_none().whitespace_nowrap())
            },
          )
          .child(Label::new(name).text_size(FILE_FONT_SIZE).min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis()),
      )
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
    let initial: SharedString = detail.info.author.trim().chars().take(1).flat_map(char::to_uppercase).collect::<String>().into();
    let unfolded = self.message_unfolded;
    let copy = |id: String| {
      Clipboard::new(SharedString::from(format!("copy-commit-id-{id}")))
        .value(id)
        .accessibility_label("Copy the full commit id")
        .tooltip("Copy the full commit id")
    };
    let muted = theme.muted_foreground;
    // A long subject goes down onto further lines; the button stays at the top right, so the message can always be folded.
    let title = h_flex()
      .w_full()
      .gap_2()
      .px_4()
      .py_2()
      .items_start()
      .child(div().flex_1().min_w_0().font_weight(FontWeight::SEMIBOLD).whitespace_normal().child(detail.info.summary.clone()))
      .child(
        div().flex_none().child(
          Button::new("commit-message-toggle")
            .ghost()
            .xsmall()
            .icon(Icon::empty().path(if unfolded { "icons/fold-vertical.svg" } else { "icons/unfold-vertical.svg" }))
            .tooltip(if unfolded { "Fold the message" } else { "Unfold the message" })
            .on_click(cx.listener(|this, _, _, cx| {
              this.message_unfolded = !this.message_unfolded;
              cx.notify();
            })),
        ),
      );
    // The message as it was written (a fixed-width font keeps lists and wrapped lines as typed), in a box that scrolls.
    let body_box = (!detail.body.is_empty()).then(|| {
      let height = if unfolded { MESSAGE_UNFOLDED_HEIGHT } else { MESSAGE_FOLDED_HEIGHT };
      div()
        .relative()
        .w_full()
        .min_w_0()
        .max_h(height)
        .overflow_hidden()
        .border_y_1()
        .border_color(palette.border)
        .bg(palette.surface)
        .child(
          div().id("commit-message").w_full().max_h(height).overflow_y_scroll().track_scroll(&self.message_scroll).child(
            div()
              .w_full()
              .min_w_0()
              .px_4()
              .py_2()
              .font_family(theme.mono_font_family.clone())
              .text_size(FONT_SIZE_SM)
              .whitespace_normal()
              .child(detail.body.clone()),
          ),
        )
        .child(
          div()
            .absolute()
            .inset_0()
            .child(Scrollbar::new(&self.message_scroll).id("commit-message-scrollbar").axis(ScrollbarAxis::Vertical).viewport_from_layout()),
        )
    });
    let author = |text: String| {
      h_flex()
        .gap_2()
        .items_center()
        .child(Avatar::new().name(initial.clone()).xsmall())
        .child(Label::new(text).text_size(FONT_SIZE_SM).text_color(muted))
    };
    let stats = h_flex()
      .gap_3()
      .items_center()
      .child(Label::new(format!("+{}", detail.additions)).text_size(FONT_SIZE_SM).text_color(palette.new))
      .child(Label::new(format!("-{}", detail.deletions)).text_size(FONT_SIZE_SM).text_color(palette.deleted));
    let footer = if unfolded {
      let name =
        if detail.author_email.is_empty() { detail.info.author.clone() } else { format!("{} <{}>", detail.info.author, detail.author_email) };
      v_flex()
        .gap_1()
        .px_4()
        .py_2()
        .child(author(name))
        .child(
          h_flex()
            .gap_2()
            .items_center()
            .child(Icon::empty().path("icons/git-commit-horizontal.svg").size_4().text_color(muted))
            .child(Label::new(detail.info.id.clone()).text_size(FONT_SIZE_SM).text_color(muted))
            .child(copy(id.clone())),
        )
        .child(
          h_flex()
            .gap_2()
            .items_center()
            .child(Icon::empty().path("icons/diff.svg").size_4().text_color(muted))
            .child(Label::new(format!("{} added lines", detail.additions)).text_size(FONT_SIZE_SM).text_color(palette.new))
            .child(Label::new(format!("{} removed lines", detail.deletions)).text_size(FONT_SIZE_SM).text_color(palette.deleted)),
        )
        .into_any_element()
    } else {
      h_flex()
        .gap_3()
        .px_4()
        .py_1p5()
        .items_center()
        .justify_between()
        .child(
          h_flex()
            .gap_2()
            .items_center()
            .child(author(detail.info.author.clone()))
            .child(Icon::empty().path("icons/git-commit-horizontal.svg").size_4().text_color(muted))
            .child(Label::new(detail.info.short.clone()).text_size(FONT_SIZE_SM).text_color(muted))
            .child(copy(id.clone())),
        )
        .child(stats)
        .into_any_element()
    };
    let header = v_flex()
      .flex_none()
      .w_full()
      .min_w_0()
      .overflow_hidden()
      .border_b_1()
      .border_color(palette.border)
      .child(title)
      .children(body_box)
      .child(footer);
    let count = detail.files.len();
    // No border of its own on the right: the handle of the split draws the line between the list and the diff.
    let files = v_flex()
      .size_full()
      .child(
        Label::new(if count == 1 { "1 changed file".to_string() } else { format!("{count} changed files") })
          .px_3()
          .py_1p5()
          .border_b_1()
          .border_color(palette.border),
      )
      .child(
        div()
          .relative()
          .flex_1()
          .min_h_0()
          .child(
            v_virtual_list(cx.entity(), "commit-files", self.file_sizes.clone(), |this, range, _, cx| {
              let Load::Ready(detail) = &this.detail else {
                return Vec::new();
              };
              let files: Vec<ws_git::Change> = detail.files.get(range).map(<[ws_git::Change]>::to_vec).unwrap_or_default();
              let width = this.files_scroll.bounds().size.width;
              files.iter().map(|change| this.file_row(change, width, cx)).collect()
            })
            .track_scroll(&self.files_scroll),
          )
          .child(
            div().absolute().inset_0().child(
              Scrollbar::new(&self.files_scroll)
                .id("commit-files-scrollbar")
                .axis(ScrollbarAxis::Vertical)
                .mode(ScrollbarMode::Scrolling)
                .viewport_from_layout(),
            ),
          ),
      );
    let diff = match &self.diff {
      Load::Loading => spinner(),
      Load::Failed(error) => message(error.clone(), false),
      Load::Ready(table) => v_flex()
        .size_full()
        .min_h_0()
        // The path on the left, the buttons that move through the changes and set the diff at the right end.
        .child(
          h_flex()
            .flex_none()
            .gap_2()
            .px_4()
            .py_1p5()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(palette.border)
            .child(
              div()
                .flex_1()
                .min_w_0()
                .children(self.selected.as_ref().map(|file| crate::components::file_path_bar("commit", &file.to_string_lossy(), None, cx))),
            )
            .child(div().flex_none().child(table_controls(table))),
        )
        .child(div().flex_1().min_h_0().min_w_0().child(table.clone()))
        .into_any_element(),
    };
    v_flex().size_full().min_w_0().bg(theme.background).child(header).child(
      div().flex_1().min_h_0().child(
        h_resizable("commit-files-split")
          .child(resizable_panel().size(FILES_WIDTH).size_range(FILES_MIN_WIDTH..FILES_MAX_WIDTH).fixed().child(files))
          .child(resizable_panel().child(div().size_full().min_w_0().child(diff))),
      ),
    )
  }
}
