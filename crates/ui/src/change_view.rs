//! The changes of one file of the Git tab, shown in the main area in place of the editor: the path on top,
//! the diff below. Choosing another file replaces it; no tab is added.

use crate::{
  commit_view::Load,
  diff_table::{prepare, table_controls, DiffTable, OpenLine, Staging, ToggleGroup},
  settings::SettingsModel,
  theme::colors,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    label::Label,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Sizable as _,
  },
  *,
};
use std::{
  ops::Range,
  path::{Path, PathBuf},
  sync::Arc,
};

use ws_git::{StageDiff, StageSide};

const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

enum ChangeDiff {
  Text(Entity<DiffTable>),
  Image(ImageChanges),
}

/// What the index holds of a file's changes: both of its diffs, and for each run of changed lines of the shown diff whether it is staged.
#[derive(Clone, PartialEq, Eq)]
struct IndexInfo {
  unstaged: Arc<StageDiff>,
  staged: Arc<StageDiff>,
  states: Vec<Staging>,
  file: Staging,
}

/// Where a run of changed lines changes the old and the new file.
type Spans = (Range<u32>, Range<u32>);

/// The hunks of a diff that a request to the index covers, with the diff they belong to.
pub(crate) type HunkChoice = (Arc<StageDiff>, Vec<usize>);

impl IndexInfo {
  /// A diff that cannot be cut into hunks (a new or deleted file, a mode change) stands for every run when it has changes.
  fn covers(diff: &StageDiff, meets: impl Fn(&ws_git::HunkSpan) -> bool, spans: &[ws_git::HunkSpan]) -> bool {
    if diff.hunks().next().is_none() {
      diff.has_changes()
    } else {
      spans.iter().any(meets)
    }
  }

  fn new(unstaged: StageDiff, staged: StageDiff, groups: &[Spans]) -> Self {
    let (unstaged_spans, staged_spans) = (unstaged.changed_spans(), staged.changed_spans());
    let states = groups
      .iter()
      .map(|(old, new)| {
        let unstaged_part = Self::covers(&unstaged, |span| span.meets_new(new), &unstaged_spans);
        let staged_part = Self::covers(&staged, |span| span.meets_old(old), &staged_spans);
        match (unstaged_part, staged_part) {
          (true, true) => Staging::Partial,
          (true, false) => Staging::Unstaged,
          _ => Staging::Staged,
        }
      })
      .collect();
    let file = match (unstaged.has_changes(), staged.has_changes()) {
      (true, true) => Staging::Partial,
      (true, false) => Staging::Unstaged,
      _ => Staging::Staged,
    };
    Self { unstaged: Arc::new(unstaged), staged: Arc::new(staged), states, file }
  }

  /// What clicking the check box of a run of changed lines asks of the index: a staged run is unstaged, any other is staged, by the
  /// hunks of the diff that touch it (the whole file when the diff has none).
  fn request(&self, group: usize, groups: &[Spans]) -> (StageSide, Option<HunkChoice>) {
    let Some((old, new)) = groups.get(group) else { return (StageSide::Unstaged, None) };
    let staged = self.states.get(group) == Some(&Staging::Staged);
    let (side, diff) = if staged { (StageSide::Staged, &self.staged) } else { (StageSide::Unstaged, &self.unstaged) };
    let positions: Vec<usize> = diff
      .changed_spans()
      .iter()
      .enumerate()
      .filter(|(_, span)| if staged { span.meets_old(old) } else { span.meets_new(new) })
      .map(|(position, _)| position)
      .collect();
    (side, (!positions.is_empty()).then(|| (diff.clone(), positions)))
  }
}

struct ImageChanges {
  original: Option<Arc<RenderImage>>,
  current: Option<Arc<RenderImage>>,
}

impl ImageChanges {
  fn format(path: &Path) -> Option<ImageFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
      "png" => Some(ImageFormat::Png),
      "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
      "webp" => Some(ImageFormat::Webp),
      "bmp" => Some(ImageFormat::Bmp),
      "tiff" | "tif" => Some(ImageFormat::Tiff),
      _ => None,
    }
  }

  fn load(root: &Path, file: &Path, decode: impl Fn(Vec<u8>) -> Result<Arc<RenderImage>, String>) -> Result<Self, String> {
    let versions = ws_git::file_versions(root, file, MAX_IMAGE_BYTES)?;
    Ok(Self { original: versions.original.map(&decode).transpose()?, current: versions.current.map(decode).transpose()? })
  }

  fn view(&self, cx: &App) -> impl IntoElement {
    let palette = colors(cx);
    let pane = |id, title, image: &Arc<RenderImage>, color| {
      v_flex().id(id).flex_1().min_w_0().min_h_0().gap_2().items_center().child(Label::new(title).text_color(color)).child(
        div()
          .flex_1()
          .min_h_0()
          .w_full()
          .relative()
          .overflow_hidden()
          .child(img(image.clone()).absolute().top_0().left_0().size_full().object_fit(ObjectFit::ScaleDown)),
      )
    };
    let modified = self.original.is_some() && self.current.is_some();
    h_flex()
      .flex_1()
      .min_h_0()
      .w_full()
      .items_stretch()
      .gap_4()
      .p_4()
      .children(self.original.as_ref().map(|image| pane("original-image", if modified { "Before" } else { "Deleted" }, image, palette.deleted)))
      .children(self.current.as_ref().map(|image| pane("current-image", if modified { "After" } else { "Added" }, image, palette.new)))
  }
}

pub(crate) struct ChangeView {
  /// The settings of the profile, for the size of code.
  settings: Entity<SettingsModel>,
  root: PathBuf,
  file: PathBuf,
  /// Where the file was before, when it was renamed.
  from: Option<PathBuf>,
  /// What kind of change the file has, for the mark at the end of the path.
  kind: Option<ws_git::ChangeKind>,
  diff: Load<ChangeDiff>,
  /// What the index holds of the file, and where each run of changed lines of the shown diff is; none while the file is in conflict.
  index: Option<IndexInfo>,
  spans: Vec<Spans>,
  task: Option<Task<()>>,
  show_preview: bool,
  svg_preview: Option<Load<ImageChanges>>,
  preview_task: Option<Task<()>>,
  /// The patch the shown diff was made from, to leave the diff (and its scroll position) alone while nothing changed.
  patch: String,
  /// Which read the answer being awaited belongs to.
  epoch: u64,
  _table_subscriptions: Vec<Subscription>,
}

/// A double click on a line of the diff: open the file in the editor at that line.
pub(crate) struct OpenInEditor {
  pub file: PathBuf,
  /// The line in the new version of the file, counted from 1.
  pub line: u32,
}

pub(crate) struct StageRequest {
  pub root: PathBuf,
  pub files: Vec<PathBuf>,
  pub side: StageSide,
  pub hunk: Option<HunkChoice>,
}

impl EventEmitter<StageRequest> for ChangeView {}

impl EventEmitter<OpenInEditor> for ChangeView {}

impl ChangeView {
  pub fn new(settings: Entity<SettingsModel>) -> Self {
    Self {
      settings,
      root: PathBuf::new(),
      file: PathBuf::new(),
      from: None,
      kind: None,
      diff: Load::Loading,
      index: None,
      spans: Vec::new(),
      task: None,
      show_preview: false,
      svg_preview: None,
      preview_task: None,
      patch: String::new(),
      epoch: 0,
      _table_subscriptions: Vec::new(),
    }
  }

  /// Shows the changes of another file of the repository at `root`.
  /// Always starts from a read: the view keeps the last file it showed after the file was committed or deselected, and showing
  /// the same file again must not bring back what it looked like then.
  pub fn show(&mut self, root: PathBuf, file: PathBuf, change: Option<&ws_git::Change>, cx: &mut Context<Self>) {
    self.root = root;
    self.file = file;
    self.from = change.and_then(|change| change.from.clone());
    self.kind = change.map(|change| change.kind);
    self.index = None;
    self.spans.clear();
    self.diff = Load::Loading;
    self.show_preview = false;
    self.svg_preview = None;
    self.preview_task = None;
    self.patch.clear();
    self.reload(cx);
    cx.notify();
  }

  fn stage_paths(&self) -> Vec<PathBuf> {
    std::iter::once(self.file.clone()).chain(self.from.clone()).collect()
  }

  fn stage(&self, side: StageSide, hunk: Option<HunkChoice>, cx: &mut Context<Self>) {
    cx.emit(StageRequest { root: self.root.clone(), files: self.stage_paths(), side, hunk });
  }

  /// The check box of a run of changed lines of the diff was clicked.
  fn toggle_group(&self, group: usize, cx: &mut Context<Self>) {
    if let Some(index) = &self.index {
      let (side, hunk) = index.request(group, &self.spans);
      self.stage(side, hunk, cx);
    }
  }

  /// The check box of the whole file: a staged file is unstaged, any other is staged.
  fn toggle_file(&self, cx: &mut Context<Self>) {
    if let Some(index) = &self.index {
      self.stage(if index.file == Staging::Staged { StageSide::Staged } else { StageSide::Unstaged }, None, cx);
    }
  }

  /// Reads the file's changes again, and redraws only if they differ from what is shown.
  pub fn reload(&mut self, cx: &mut Context<Self>) {
    self.epoch += 1;
    let (epoch, root, file, from) = (self.epoch, self.root.clone(), self.file.clone(), self.from.clone());
    let files = self.stage_paths();
    let stageable = self.kind != Some(ws_git::ChangeKind::Conflicted);
    let (previous_index, previous_spans) = (self.index.clone(), self.spans.clone());
    let previous = matches!(self.diff, Load::Ready(_)).then(|| self.patch.clone());
    let renderer = cx.svg_renderer();
    let theme = crate::diff_view::highlight_theme(cx);
    let work = cx.background_executor().spawn(async move {
      let patch = match &from {
        Some(from) => ws_git::renamed_file_diff(&root, &file, from)?,
        None => ws_git::file_diff(&root, &file)?,
      };
      let same_patch = previous.as_deref() == Some(patch.as_str());
      let (prepared, spans) = if same_patch {
        (None, previous_spans)
      } else {
        crate::diff_view::warm(&theme, &patch);
        let prepared = prepare(&theme, &patch, &file);
        let spans = prepared.as_ref().map(|prepared| prepared.spans()).unwrap_or_default();
        (Some(prepared), spans)
      };
      // A file that cannot be read from the index is shown without check boxes rather than not at all.
      let index = stageable
        .then(|| Some((ws_git::staging_diff(&root, &files, StageSide::Unstaged).ok()?, ws_git::staging_diff(&root, &files, StageSide::Staged).ok()?)))
        .flatten()
        .map(|(unstaged, staged)| IndexInfo::new(unstaged, staged, &spans));
      if same_patch && index == previous_index {
        return Ok(None);
      }
      let image = prepared
        .as_ref()
        .and_then(|_| ImageChanges::format(&file))
        .filter(|_| patch.lines().any(|line| line.starts_with("Binary files ")))
        .map(|format| {
          ImageChanges::load(&root, &file, |bytes| {
            Image::from_bytes(format, bytes).to_image_data(renderer.clone()).map_err(|error| error.to_string())
          })
        })
        .transpose();
      Ok::<_, String>(Some((patch, same_patch, image, prepared, spans, index)))
    });
    self.task = Some(cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        if this.epoch != epoch {
          return;
        }
        this.task = None;
        match result {
          Ok(None) => return,
          Ok(Some((_, true, _, _, _, index))) => {
            // Only the index changed: the diff, and its scroll position, stay.
            this.index = index;
            this.show_staging(cx);
          }
          Ok(Some((patch, _, image, prepared, spans, index))) => {
            this.diff = match image {
              Ok(Some(image)) if image.original.is_some() || image.current.is_some() => Load::Ready(ChangeDiff::Image(image)),
              Err(error) => Load::Failed(format!("Could not preview the image: {error}").into()),
              _ => match prepared {
                Some(Ok(prepared)) => {
                  let settings = this.settings.clone();
                  let table = cx.new(|cx| DiffTable::new(prepared, settings, cx));
                  this._table_subscriptions = vec![
                    cx.subscribe(&table, |this, _, event: &OpenLine, cx| {
                      cx.emit(OpenInEditor { file: this.file.clone(), line: event.0 });
                    }),
                    cx.subscribe(&table, |this, _, event: &ToggleGroup, cx| this.toggle_group(event.0, cx)),
                  ];
                  Load::Ready(ChangeDiff::Text(table))
                }
                Some(Err(message)) => Load::Failed(message.into()),
                None => Load::Loading,
              },
            };
            this.patch = patch;
            this.spans = spans;
            this.index = index;
            this.preview_task = None;
            this.svg_preview = None;
            this.show_staging(cx);
            if this.show_preview && matches!(this.diff, Load::Ready(ChangeDiff::Text(_))) {
              this.load_svg_preview(cx);
            }
          }
          Err(message) => this.diff = Load::Failed(message.into()),
        }
        cx.notify();
      });
    }));
  }

  /// Hands the staging of each run of changed lines to the diff table, which draws the check boxes.
  fn show_staging(&self, cx: &mut Context<Self>) {
    if let Load::Ready(ChangeDiff::Text(table)) = &self.diff {
      let states = self.index.as_ref().map(|index| index.states.clone());
      table.update(cx, |table, cx| table.set_staging(states, cx));
    }
  }

  fn load_svg_preview(&mut self, cx: &mut Context<Self>) {
    let (root, file) = (self.root.clone(), self.file.clone());
    let renderer = cx.svg_renderer();
    self.svg_preview = Some(Load::Loading);
    let work = cx
      .background_executor()
      .spawn(async move { ImageChanges::load(&root, &file, |bytes| renderer.render_single_frame(&bytes, 1.0).map_err(|error| error.to_string())) });
    self.preview_task = Some(cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        this.preview_task = None;
        this.svg_preview = Some(match result {
          Ok(images) if images.original.is_some() || images.current.is_some() => Load::Ready(images),
          Ok(_) => Load::Failed("No SVG versions are available to preview.".into()),
          Err(error) => Load::Failed(format!("Could not preview the SVG: {error}").into()),
        });
        cx.notify();
      });
    }));
  }
}

impl Render for ChangeView {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let can_preview =
      self.file.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("svg")) && matches!(self.diff, Load::Ready(ChangeDiff::Text(_)));
    // One box for the whole file, as in the list of changes: ticked when everything of it is staged, a dash when some of it is.
    let file_box = self.index.as_ref().map(|index| {
      Checkbox::new("stage-file")
        .label("Staged")
        .checked(index.file == Staging::Staged)
        .indeterminate(index.file == Staging::Partial)
        .tooltip(if index.file == Staging::Staged { "Unstage this file" } else { "Stage this file" })
        .on_click(cx.listener(|this, _: &bool, _, cx| this.toggle_file(cx)))
    });
    let header = h_flex()
      .flex_none()
      .gap_3()
      .px_4()
      .py_1p5()
      .items_center()
      .border_b_1()
      .border_color(palette.border)
      .child(crate::components::file_path_bar(
        "change",
        &self.file.to_string_lossy(),
        self.from.as_deref().map(|from| from.to_string_lossy()).as_deref(),
        cx,
      ))
      .child(div().flex_1())
      .children(file_box)
      .when(can_preview, |header| {
        header.child(Button::new("toggle-svg-change-preview").ghost().small().label(if self.show_preview { "Source" } else { "Preview" }).on_click(
          cx.listener(|this, _, _, cx| {
            this.show_preview = !this.show_preview;
            if this.show_preview && this.svg_preview.is_none() {
              this.load_svg_preview(cx);
            }
            cx.notify();
          }),
        ))
      })
      // The previous/next/options buttons of a text diff sit at the right end.
      .children(match &self.diff {
        Load::Ready(ChangeDiff::Text(table)) if !self.show_preview => Some(table_controls(table)),
        _ => None,
      })
      // The mark of the change, as in the Changes list.
      .children(self.kind.map(|kind| div().child(crate::components::change_kind_mark(kind, cx))));
    let loading = || {
      h_flex()
        .gap_2()
        .p_4()
        .items_center()
        .child(Spinner::new().small())
        .child(Label::new("Loading…").text_color(theme.muted_foreground))
        .into_any_element()
    };
    let failed = |message: &SharedString| Label::new(message.clone()).p_4().text_color(theme.muted_foreground).into_any_element();
    let body = match &self.diff {
      Load::Loading => loading(),
      Load::Failed(message) => failed(message),
      Load::Ready(ChangeDiff::Image(images)) => images.view(cx).into_any_element(),
      Load::Ready(ChangeDiff::Text(_)) if self.show_preview => match &self.svg_preview {
        Some(Load::Ready(images)) => images.view(cx).into_any_element(),
        Some(Load::Failed(message)) => failed(message),
        _ => loading(),
      },
      Load::Ready(ChangeDiff::Text(table)) => div().size_full().child(table.clone()).into_any_element(),
    };
    v_flex().size_full().bg(theme.background).child(header).child(v_flex().flex_1().min_h_0().child(body))
  }
}
