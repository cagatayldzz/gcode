//! Quick Open (⌘P): type a few letters of a file name or path and open the best match.

use crate::theme::FONT_SIZE_SM;
use gpui_kit::{
  component::{
    command::{Command, CommandItem, CommandState},
    h_flex,
    label::Label,
    v_flex, ActiveTheme as _,
  },
  *,
};
use std::{path::PathBuf, sync::Arc};

/// How many matches the list shows.
const RESULT_LIMIT: usize = 50;

/// Sent with the full path of the file the user chose.
pub(crate) struct FileChosen(pub PathBuf);

impl EventEmitter<FileChosen> for QuickOpen {}

pub(crate) struct QuickOpen {
  root: PathBuf,
  /// Every file of the project as a path below the project folder; empty until the project index has been read.
  files: Arc<Vec<SharedString>>,
  results: Vec<SharedString>,
  query: SharedString,
  state: Entity<CommandState>,
  /// Numbers the searches, so a slow one never replaces the answer to a newer query.
  epoch: usize,
  _load: Task<()>,
  search: Option<Task<()>>,
}

impl QuickOpen {
  pub fn new(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let state = cx.new(|cx| CommandState::new(window, cx));
    let index_root = root.clone();
    let load = cx.spawn(async move |this, cx| {
      let work = cx.background_executor().spawn(async move {
        let index = ws_index::ProjectIndex::open(&ws_storage::index_dir(), &index_root).index;
        index.files().iter().map(|file| SharedString::from(file.path.to_string_lossy().into_owned())).collect::<Vec<_>>()
      });
      let files = work.await;
      let _ = this.update(cx, |this, cx| {
        this.files = Arc::new(files);
        this.run_search(cx);
      });
    });
    Self { root, files: Arc::new(Vec::new()), results: Vec::new(), query: SharedString::default(), state, epoch: 0, _load: load, search: None }
  }

  pub fn focus(&self, window: &mut Window, cx: &mut App) {
    self.state.update(cx, |state, cx| state.focus(window, cx));
  }

  fn set_query(&mut self, query: &str, cx: &mut Context<Self>) {
    self.query = query.to_owned().into();
    self.run_search(cx);
  }

  /// Ranks the files for the current query without blocking the interface.
  fn run_search(&mut self, cx: &mut Context<Self>) {
    self.epoch += 1;
    let epoch = self.epoch;
    let files = self.files.clone();
    let query = self.query.clone();
    self.search = Some(cx.spawn(async move |this, cx| {
      let work = cx.background_executor().spawn(async move {
        let found = ws_index::fuzzy::rank(&query, files.iter().map(|path| path.as_ref()), RESULT_LIMIT);
        found.into_iter().map(|index| files[index].clone()).collect::<Vec<_>>()
      });
      let results = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.epoch == epoch {
          this.results = results;
          cx.notify();
        }
      });
    }));
  }

  fn confirm(&mut self, row: usize, cx: &mut Context<Self>) {
    if let Some(path) = self.results.get(row) {
      cx.emit(FileChosen(self.root.join(path.as_ref() as &str)));
    }
  }
}

impl Render for QuickOpen {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let this = cx.entity().downgrade();
    let confirming = this.clone();
    let items = self.results.iter().map(|path| {
      let (folder, name) = match path.rsplit_once('/') {
        Some((folder, name)) => (SharedString::from(folder.to_owned()), SharedString::from(name.to_owned())),
        None => (SharedString::default(), path.clone()),
      };
      CommandItem::new().label(path.clone()).child(move |_, _| {
        h_flex()
          .gap_2()
          .items_baseline()
          .min_w_0()
          .child(Label::new(name.clone()).whitespace_nowrap())
          .child(Label::new(folder.clone()).text_size(FONT_SIZE_SM).text_color(muted).overflow_hidden().whitespace_nowrap().text_ellipsis())
      })
    });
    v_flex().child(
      Command::new(&self.state)
        .filterable(false)
        .bordered(false)
        .placeholder("Go to file")
        .items(items)
        .empty(move |_, _, _| Label::new("No file matches.").text_color(muted).p_2())
        .on_query(move |query, _, cx| {
          let _ = this.update(cx, |this, cx| this.set_query(query, cx));
        })
        .on_confirm(move |index, _, cx| {
          let _ = confirming.update(cx, |this, cx| this.confirm(index.row, cx));
        }),
    )
  }
}
