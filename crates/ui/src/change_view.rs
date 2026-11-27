//! The changes of one file of the Git tab, shown in the main area in place of the editor: the path on top,
//! the diff below. Choosing another file replaces it; no tab is added.

use crate::{
  commit_view::Load,
  diff_table::{prepare, table_controls, DiffTable, OpenLine},
  settings::SettingsModel,
  theme::colors,
};
use gpui_kit::{base::Selectable as _, prelude::FluentBuilder as _};
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    menu::{DropdownMenu as _, PopupMenuItem},
    spinner::Spinner,
    v_flex, ActiveTheme as _, Sizable as _,
  },
  *,
};
use std::{
  path::{Path, PathBuf},
  sync::Arc,
};

use ws_git::{StageDiff, StageSide};

const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

enum ChangeDiff {
  Empty,
  Text(Entity<DiffTable>),
  Image(ImageChanges),
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
  stage_side: Option<StageSide>,
  stage_diff: Option<Arc<StageDiff>>,
  task: Option<Task<()>>,
  show_preview: bool,
  svg_preview: Option<Load<ImageChanges>>,
  preview_task: Option<Task<()>>,
  /// The patch the shown diff was made from, to leave the diff (and its scroll position) alone while nothing changed.
  patch: String,
  /// Which read the answer being awaited belongs to.
  epoch: u64,
  _table_subscription: Option<Subscription>,
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
  pub hunk: Option<(Arc<StageDiff>, usize)>,
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
      stage_side: None,
      stage_diff: None,
      task: None,
      show_preview: false,
      svg_preview: None,
      preview_task: None,
      patch: String::new(),
      epoch: 0,
      _table_subscription: None,
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
    self.stage_diff = None;
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

  fn select_side(&mut self, side: Option<StageSide>, cx: &mut Context<Self>) {
    if self.stage_side != side {
      self.stage_side = side;
      self.stage_diff = None;
      self.diff = Load::Loading;
      self.patch.clear();
      self.show_preview = false;
      self.svg_preview = None;
      self.preview_task = None;
      self.reload(cx);
      cx.notify();
    }
  }

  fn stage(&self, side: StageSide, hunk: Option<(Arc<StageDiff>, usize)>, cx: &mut Context<Self>) {
    cx.emit(StageRequest { root: self.root.clone(), files: self.stage_paths(), side, hunk });
  }

  /// Reads the file's changes again, and redraws only if they differ from what is shown.
  pub fn reload(&mut self, cx: &mut Context<Self>) {
    self.epoch += 1;
    let (epoch, root, file, from) = (self.epoch, self.root.clone(), self.file.clone(), self.from.clone());
    let side = self.stage_side;
    let files = self.stage_paths();
    let previous_stage = self.stage_diff.clone();
    let previous = matches!(self.diff, Load::Ready(_)).then(|| self.patch.clone());
    let renderer = cx.svg_renderer();
    let theme = crate::diff_view::highlight_theme(cx);
    let work = cx.background_executor().spawn(async move {
      let stage_diff = side.map(|side| ws_git::staging_diff(&root, &files, side)).transpose()?;
      let patch = match (&stage_diff, &from) {
        (Some(diff), _) if diff.is_untracked() => ws_git::file_diff(&root, &file)?,
        (Some(diff), _) => diff.text(),
        (None, Some(from)) => ws_git::renamed_file_diff(&root, &file, from)?,
        (None, None) => ws_git::file_diff(&root, &file)?,
      };
      if previous.as_deref() == Some(patch.as_str()) && stage_diff.as_ref() == previous_stage.as_deref() {
        return Ok(None);
      }
      crate::diff_view::warm(&theme, &patch);
      let prepared = prepare(&theme, &patch, &file);
      let image = ImageChanges::format(&file)
        .filter(|_| stage_diff.is_none() && patch.lines().any(|line| line.starts_with("Binary files ")))
        .map(|format| {
          ImageChanges::load(&root, &file, |bytes| {
            Image::from_bytes(format, bytes).to_image_data(renderer.clone()).map_err(|error| error.to_string())
          })
        })
        .transpose();
      Ok::<_, String>(Some((patch, image, prepared, stage_diff)))
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
          Ok(Some((patch, image, prepared, stage_diff))) => {
            this.diff = match image {
              _ if stage_diff.is_some() && patch.is_empty() => Load::Ready(ChangeDiff::Empty),
              Ok(Some(image)) if image.original.is_some() || image.current.is_some() => Load::Ready(ChangeDiff::Image(image)),
              Err(error) => Load::Failed(format!("Could not preview the image: {error}").into()),
              _ => match prepared {
                Ok(prepared) => {
                  let settings = this.settings.clone();
                  let table = cx.new(|cx| DiffTable::new(prepared, settings, cx));
                  this._table_subscription = Some(cx.subscribe(&table, |this, _, event: &OpenLine, cx| {
                    cx.emit(OpenInEditor { file: this.file.clone(), line: event.0 });
                  }));
                  Load::Ready(ChangeDiff::Text(table))
                }
                Err(message) => Load::Failed(message.into()),
              },
            };
            this.patch = patch;
            this.stage_diff = stage_diff.map(Arc::new);
            this.preview_task = None;
            this.svg_preview = None;
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
    let can_preview = self.stage_side.is_none()
      && self.file.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
      && matches!(self.diff, Load::Ready(ChangeDiff::Text(_)));
    let header = h_flex()
      .flex_none()
      .px_4()
      .py_1p5()
      .border_b_1()
      .border_color(palette.border)
      .child(crate::components::file_path_bar(
        "change",
        &self.file.to_string_lossy(),
        self.from.as_deref().map(|from| from.to_string_lossy()).as_deref(),
        cx,
      ))
      .child(div().flex_1())
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
      // The previous/next/options buttons of a text diff sit at the right end of the path.
      .children(match &self.diff {
        Load::Ready(ChangeDiff::Text(table)) if !self.show_preview => Some(table_controls(table)),
        _ => None,
      })
      // The mark of the change, as in the Changes list.
      .children(self.kind.map(|kind| div().ml_2().child(crate::components::change_kind_mark(kind, cx))));
    let mut index_bar = h_flex().gap_2().px_4().py_1().border_b_1().border_color(palette.border);
    for (key, label, side) in [
      ("all-diff", "All Changes", None),
      ("unstaged-diff", "Unstaged", Some(StageSide::Unstaged)),
      ("staged-diff", "Staged", Some(StageSide::Staged)),
    ] {
      index_bar = index_bar.child(
        Button::new(key)
          .small()
          .ghost()
          .selected(self.stage_side == side)
          .label(label)
          .on_click(cx.listener(move |this, _, _, cx| this.select_side(side, cx))),
      );
    }
    if let Some(side) =
      self.stage_side.filter(|_| self.kind != Some(ws_git::ChangeKind::Conflicted) && self.stage_diff.as_ref().is_some_and(|diff| diff.has_changes()))
    {
      let (file_label, hunk_label) = match side {
        StageSide::Unstaged => ("Stage File", "Stage Hunk"),
        StageSide::Staged => ("Unstage File", "Unstage Hunk"),
      };
      index_bar = index_bar
        .child(div().flex_1())
        .child(Button::new("change-index-file").small().label(file_label).on_click(cx.listener(move |this, _, _, cx| this.stage(side, None, cx))));
      if let Some(diff) = self.stage_diff.clone().filter(|diff| diff.hunks().next().is_some()) {
        let owner = cx.entity().downgrade();
        let root = self.root.clone();
        let files = self.stage_paths();
        index_bar = index_bar.child(Button::new("change-index-hunk").small().label(hunk_label).dropdown_menu(move |menu, _, _| {
          diff.hunks().enumerate().fold(menu, |menu, (number, header)| {
            let (owner, diff, root, files) = (owner.clone(), diff.clone(), root.clone(), files.clone());
            menu.item(PopupMenuItem::new(SharedString::from(header.to_owned())).on_click(move |_, _, cx| {
              let _ = owner
                .update(cx, |_, cx| cx.emit(StageRequest { root: root.clone(), files: files.clone(), side, hunk: Some((diff.clone(), number)) }));
            }))
          })
        }));
      }
    }
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
      // The path is in the header above: the diff does not repeat it.
      Load::Ready(ChangeDiff::Empty) => {
        Label::new(if self.stage_side == Some(StageSide::Staged) { "No staged changes in this file." } else { "No unstaged changes in this file." })
          .p_4()
          .text_color(theme.muted_foreground)
          .into_any_element()
      }
      Load::Ready(ChangeDiff::Text(table)) => div().size_full().child(table.clone()).into_any_element(),
    };
    v_flex().size_full().bg(theme.background).child(header).child(index_bar).child(v_flex().flex_1().min_h_0().child(body))
  }
}
