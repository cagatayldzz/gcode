//! The changes of one file of the Git tab, shown in the main area in place of the editor, like GitHub Desktop: the path on top,
//! the diff below. Choosing another file replaces it; no tab is added.

use crate::{
  commit_view::Load,
  diff_view::ShownDiff,
  settings::{code_font_size, SettingsModel},
  theme::{colors, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    diff::DiffFile,
    h_flex,
    label::Label,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Sizable as _,
  },
  *,
};
use std::{
  path::{Path, PathBuf},
  sync::Arc,
};

const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

enum ChangeDiff {
  Text(ShownDiff),
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
    let versions = vekro_git::file_versions(root, file, MAX_IMAGE_BYTES)?;
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
  diff: Load<ChangeDiff>,
  task: Option<Task<()>>,
  show_preview: bool,
  svg_preview: Option<Load<ImageChanges>>,
  preview_task: Option<Task<()>>,
  /// The patch the shown diff was made from, to leave the diff (and its scroll position) alone while nothing changed.
  patch: String,
  /// Which read the answer being awaited belongs to.
  epoch: u64,
}

impl ChangeView {
  pub fn new(settings: Entity<SettingsModel>) -> Self {
    Self {
      settings,
      root: PathBuf::new(),
      file: PathBuf::new(),
      diff: Load::Loading,
      task: None,
      show_preview: false,
      svg_preview: None,
      preview_task: None,
      patch: String::new(),
      epoch: 0,
    }
  }

  /// Shows the changes of another file of the repository at `root`.
  pub fn show(&mut self, root: PathBuf, file: PathBuf, cx: &mut Context<Self>) {
    if self.root == root && self.file == file {
      return;
    }
    self.root = root;
    self.file = file;
    self.diff = Load::Loading;
    self.show_preview = false;
    self.svg_preview = None;
    self.preview_task = None;
    self.patch.clear();
    self.reload(cx);
    cx.notify();
  }

  /// Reads the file's changes again, and redraws only if they differ from what is shown.
  pub fn reload(&mut self, cx: &mut Context<Self>) {
    self.epoch += 1;
    let (epoch, root, file) = (self.epoch, self.root.clone(), self.file.clone());
    let previous = matches!(self.diff, Load::Ready(_)).then(|| self.patch.clone());
    let renderer = cx.svg_renderer();
    let work = cx.background_executor().spawn(async move {
      let patch = vekro_git::file_diff(&root, &file)?;
      if previous.as_deref() == Some(patch.as_str()) {
        return Ok(None);
      }
      let image = ImageChanges::format(&file)
        .filter(|_| patch.lines().any(|line| line.starts_with("Binary files ")))
        .map(|format| {
          ImageChanges::load(&root, &file, |bytes| {
            Image::from_bytes(format, bytes).to_image_data(renderer.clone()).map_err(|error| error.to_string())
          })
        })
        .transpose();
      Ok::<_, String>(Some((patch, image)))
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
          Ok(Some((patch, image))) => {
            this.diff = match image {
              Ok(Some(image)) if image.original.is_some() || image.current.is_some() => Load::Ready(ChangeDiff::Image(image)),
              Err(error) => Load::Failed(format!("Could not preview the image: {error}").into()),
              _ => match DiffFile::parse(&patch) {
                Ok(files) if files.is_empty() => Load::Failed("This file has no text changes to show.".into()),
                Ok(files) => Load::Ready(ChangeDiff::Text(ShownDiff::new(files, &patch, cx))),
                Err(error) => Load::Failed(format!("Could not read the changes: {error}").into()),
              },
            };
            this.patch = patch;
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
    let name = self.file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let folder = self.file.parent().map(|parent| parent.display().to_string()).filter(|parent| !parent.is_empty());
    let can_preview =
      self.file.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("svg")) && matches!(self.diff, Load::Ready(ChangeDiff::Text(_)));
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
      .child(Label::new(name).text_size(FONT_SIZE_SM).font_weight(FontWeight::SEMIBOLD))
      .when(can_preview, |header| {
        header.child(div().flex_1()).child(
          Button::new("toggle-svg-change-preview").ghost().small().label(if self.show_preview { "Source" } else { "Preview" }).on_click(cx.listener(
            |this, _, _, cx| {
              this.show_preview = !this.show_preview;
              if this.show_preview && this.svg_preview.is_none() {
                this.load_svg_preview(cx);
              }
              cx.notify();
            },
          )),
        )
      });
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
      Load::Ready(ChangeDiff::Text(state)) =>
      // The path is in the header above: the diff does not repeat it.
      {
        div()
          .size_full()
          .cursor_text()
          .child(state.view(cx).header_visible(false).soft_wrap(true).border_0().text_size(code_font_size(&self.settings, cx)).size_full())
          .into_any_element()
      }
    };
    v_flex().size_full().bg(theme.background).child(header).child(v_flex().flex_1().min_h_0().child(body))
  }
}
