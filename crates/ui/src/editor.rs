//! The editor area: tabs of code editors.

use crate::close_order::CloseOrder;
use crate::components::{strip, strip_tab};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    dock::{BasePanel, Panel, PanelControl, PanelEvent},
    empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle},
    input::{Editor, EditorState, InputEvent},
    notification::Notification,
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _,
  },
  *,
};
use std::{collections::HashMap, fs, io, path::Path, path::PathBuf, sync::Arc, time::Duration};

/// Files larger than this are not opened: the editor reads the whole text.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// How many files keep their editor ready (the open one, files opened before, files prepared by hovering).
/// Files with unsaved changes are never dropped.
const MAX_READY_FILES: usize = 24;

/// The highlighter loads the syntax definitions on its first use and compiles each language the first time it is
/// drawn, which takes up to a tenth of a second. So a few lines of each common language are drawn out of sight
/// shortly after the window opens, one every `WARMUP_STEP`, and not when the user opens a file.
/// Files larger than this are not prepared by hovering: drawing them out of sight would stall the window. They open
/// on click, with their highlighting finishing in the background.
const PREFETCH_MAX_BYTES: u64 = 100 * 1024;

/// How long a prefetched editor stays drawn out of sight: long enough for a frame or two.
const PREVIEW_TIME: Duration = Duration::from_millis(500);

const WARMUP_DELAY: Duration = Duration::from_millis(1000);
const WARMUP_STEP: Duration = Duration::from_millis(250);
const WARMUP_SAMPLES: [(&str, &str); 13] = [
  ("rust", "// a\nfn main() {\n  let x: Vec<u8> = vec![1];\n  println!(\"{x:?}\");\n}\n"),
  ("toml", "# a\n[package]\nname = \"a\"\nversion = \"1.0\"\n"),
  ("markdown", "# Title\n\nText with `code` and [a link](https://a.b).\n\n- item\n"),
  ("json", "{\n  \"a\": [1, true, null],\n  \"b\": \"c\"\n}\n"),
  ("typescript", "// a\nconst x: number = 1;\nfunction f(a: string): void {}\n"),
  ("javascript", "// a\nconst x = 1;\nfunction f(a) { return `${a}`; }\n"),
  ("html", "<!doctype html>\n<div class=\"a\">text</div>\n"),
  ("css", "/* a */\n.a { color: #fff; margin: 0 }\n"),
  ("python", "# a\ndef f(a: int) -> str:\n    return f\"{a}\"\n"),
  ("bash", "# a\nset -e\necho \"$HOME\" | grep a\n"),
  ("yaml", "# a\na:\n  - b\n  - c: 1\n"),
  ("swift", "// a\nlet x: Int = 1\nfunc f() -> String { \"a\" }\n"),
  ("go", "// a\npackage main\nfunc main() { x := 1 }\n"),
];

/// A file whose editor is ready.
struct ReadyFile {
  state: Entity<EditorState>,
  dirty: bool,
  /// The editor has been drawn at least once, so its text is already highlighted.
  painted: bool,
  /// The text uses `\r\n` line endings.
  crlf: bool,
  _changes: Subscription,
  /// Repaints the status bar when the cursor moves.
  _cursor: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PreviewFormat {
  Svg,
  Icns,
}

impl PreviewFormat {
  fn for_path(path: &Path) -> Option<Self> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("svg") {
      Some(Self::Svg)
    } else if extension.eq_ignore_ascii_case("icns") {
      Some(Self::Icns)
    } else {
      None
    }
  }

  fn decode(self, path: &Path, source: Option<String>, renderer: SvgRenderer) -> Result<Arc<RenderImage>, String> {
    match self {
      Self::Svg => {
        let bytes = match source {
          Some(source) => source.into_bytes(),
          None => {
            let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
            if metadata.len() > MAX_FILE_BYTES {
              return Err("The SVG is too large to preview.".into());
            }
            fs::read(path).map_err(|error| error.to_string())?
          }
        };
        renderer.render_single_frame(&bytes, 1.0).map_err(|error| error.to_string())
      }
      Self::Icns => {
        let scratch = tempfile::tempdir().map_err(|error| error.to_string())?;
        let png = scratch.path().join("preview.png");
        let output = std::process::Command::new("/usr/bin/sips")
          .args(["-s", "format", "png", "-Z", "1024"])
          .arg(path)
          .arg("--out")
          .arg(&png)
          .output()
          .map_err(|error| error.to_string())?;
        if !output.status.success() {
          return Err("macOS could not decode the icon.".into());
        }
        let bytes = fs::read(png).map_err(|error| error.to_string())?;
        Image::from_bytes(ImageFormat::Png, bytes).to_image_data(renderer).map_err(|error| error.to_string())
      }
    }
  }
}

enum ImagePreview {
  Loading { _task: Task<()> },
  Ready(Arc<RenderImage>),
  Failed(SharedString),
}

/// The tabs of a project and the one that is shown.
#[derive(Default)]
struct Tabs {
  /// The files in the tabs, in tab order.
  items: Vec<PathBuf>,
  /// The tab opened with a single click: the next single click replaces it, until it is pinned (double click, or an edit).
  preview: Option<PathBuf>,
  active: usize,
  /// Which tab the close shortcut takes next.
  close_order: CloseOrder,
}

impl Tabs {
  fn current(&self) -> Option<&PathBuf> {
    self.items.get(self.active)
  }

  /// Shows the tab for which `same` is true, or adds `tab` and shows it.
  fn show(&mut self, tab: PathBuf, same: impl Fn(&PathBuf) -> bool) {
    self.close_order.reset();
    self.pin(&tab);
    match self.items.iter().position(same) {
      Some(index) => {
        self.items[index] = tab;
        self.active = index;
      }
      None => {
        self.items.push(tab);
        self.active = self.items.len() - 1;
      }
    }
  }

  /// Shows `tab` as a preview: an open tab is only shown, and a new one takes the place of the current preview.
  /// Returns the file whose preview tab was replaced.
  fn show_preview(&mut self, tab: PathBuf) -> Option<PathBuf> {
    self.close_order.reset();
    if let Some(index) = self.items.iter().position(|open| *open == tab) {
      self.active = index;
      return None;
    }
    let slot = self.preview.as_ref().and_then(|preview| self.items.iter().position(|open| open == preview));
    self.preview = Some(tab.clone());
    match slot {
      Some(index) => {
        self.active = index;
        Some(std::mem::replace(&mut self.items[index], tab))
      }
      None => {
        self.items.push(tab);
        self.active = self.items.len() - 1;
        None
      }
    }
  }

  /// Makes the preview tab of `path` a regular tab.
  fn pin(&mut self, path: &Path) {
    if self.preview.as_deref() == Some(path) {
      self.preview = None;
    }
  }

  /// Takes away a tab; the one on its right (else on its left) is shown.
  fn remove(&mut self, index: usize) {
    if index >= self.items.len() {
      return;
    }
    let removed = self.items.remove(index);
    self.pin(&removed);
    if self.active > index || self.active >= self.items.len() {
      self.active = self.active.saturating_sub(1);
    }
  }
}

/// What the editor area shows now, sent when it changes so the sidebar can select the same file.
pub(crate) enum ActiveTab {
  File(PathBuf),
  Nothing,
}

impl EventEmitter<ActiveTab> for EditorPanel {}

pub(crate) struct EditorPanel {
  tabs: Tabs,
  read_only: bool,
  /// Editors that are ready, by path. Reopening one is instant and keeps its unsaved text.
  files: HashMap<PathBuf, ReadyFile>,
  images: HashMap<PathBuf, ImagePreview>,
  svg_source: std::collections::HashSet<PathBuf>,
  /// What each project that is not open was showing.
  saved: HashMap<PathBuf, Tabs>,
  /// Paths in `files`, least recently used first.
  recent: Vec<PathBuf>,
  /// A few lines of code drawn out of sight so the first real file of a language does not pay for its highlighter.
  warmup: Option<Entity<EditorState>>,
  _warmup_task: Task<()>,
  /// The editor of a file the pointer rests on, drawn out of sight so its highlighting is done before the click.
  preview: Option<Entity<EditorState>>,
  preview_epoch: u64,
  focus_handle: FocusHandle,
  search_report: Option<(SharedString, Entity<EditorState>)>,
  report_active: bool,
  replacement_files: std::collections::HashSet<PathBuf>,
}

impl EditorPanel {
  pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    Self::with_read_only(false, window, cx)
  }

  pub fn side_preview(window: &mut Window, cx: &mut Context<Self>) -> Self {
    Self::with_read_only(true, window, cx)
  }

  pub fn has_tabs(&self) -> bool {
    !self.tabs.items.is_empty()
  }

  fn with_read_only(read_only: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let warmup_task = cx.spawn_in(window, async move |this, cx| {
      cx.background_executor().timer(WARMUP_DELAY).await;
      for (language, sample) in WARMUP_SAMPLES {
        let shown = this.update_in(cx, |this, window, cx| {
          this.warmup = Some(cx.new(|cx| EditorState::new(window, cx).language(language).default_value(sample)));
          cx.notify();
        });
        if shown.is_err() {
          return;
        }
        cx.background_executor().timer(WARMUP_STEP).await;
      }
      let _ = this.update(cx, |this, cx| {
        this.warmup = None;
        cx.notify();
      });
    });
    Self {
      tabs: Tabs::default(),
      read_only,
      files: HashMap::new(),
      images: HashMap::new(),
      svg_source: std::collections::HashSet::new(),
      saved: HashMap::new(),
      recent: Vec::new(),
      warmup: None,
      _warmup_task: warmup_task,
      preview: None,
      preview_epoch: 0,
      focus_handle: cx.focus_handle(),
      search_report: None,
      report_active: false,
      replacement_files: Default::default(),
    }
  }

  /// Switches to another project's editor area: the tabs of the project being left are put away and come back with
  /// it; a project not seen before starts with none.
  pub fn switch_project(&mut self, from: Option<PathBuf>, to: Option<PathBuf>, cx: &mut Context<Self>) {
    self.search_report = None;
    self.report_active = false;
    let current = std::mem::take(&mut self.tabs);
    if let Some(from) = from {
      self.saved.insert(from, current);
    }
    if let Some(tabs) = to.and_then(|to| self.saved.remove(&to)) {
      self.tabs = tabs;
    }
    self.announce(cx);
  }

  /// Whether a file inside `root` has unsaved changes.
  pub fn has_unsaved_under(&self, root: &Path) -> bool {
    self.files.iter().any(|(path, file)| file.dirty && path.starts_with(root))
  }

  /// Forgets every editor and tab of the project in `root`, which is moving: its paths are about to change.
  pub fn close_project(&mut self, root: &Path, active: bool, cx: &mut Context<Self>) {
    if active {
      self.tabs = Tabs::default();
      self.search_report = None;
      self.report_active = false;
    }
    self.saved.remove(root);
    self.files.retain(|path, _| !path.starts_with(root));
    self.images.retain(|path, _| !path.starts_with(root));
    self.svg_source.retain(|path| !path.starts_with(root));
    self.replacement_files.retain(|path| !path.starts_with(root));
    self.recent.retain(|path| !path.starts_with(root));
    self.announce(cx);
  }

  /// Tells the others what is shown and redraws.
  fn announce(&mut self, cx: &mut Context<Self>) {
    let shown = match self.tabs.current().filter(|_| !self.report_active) {
      Some(path) => ActiveTab::File(path.clone()),
      None => ActiveTab::Nothing,
    };
    cx.emit(shown);
    cx.notify();
  }

  /// Shows another tab.
  fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
    if index < self.tabs.items.len() {
      self.report_active = false;
      self.tabs.active = index;
      self.tabs.close_order.reset();
      if let Some(path) = self.tabs.current().cloned() {
        self.touch(&path);
      }
      self.announce(cx);
    }
  }

  /// The close shortcut: the shown tab first, then the others from the last to the first. Returns false when no tab is open.
  pub fn close_by_shortcut(&mut self, cx: &mut Context<Self>) -> bool {
    if self.report_active {
      self.close_report(cx);
      return true;
    }
    let Some(index) = self.tabs.close_order.next(self.tabs.items.len(), self.tabs.active) else { return false };
    self.close_tab(index, cx);
    true
  }

  /// Closes a tab. A file with unsaved text keeps it in memory: opening the file again brings it back.
  fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
    if let Some(path) = self.tabs.items.get(index) {
      self.images.remove(path);
      self.svg_source.remove(path);
    }
    self.tabs.remove(index);
    self.announce(cx);
  }

  /// Opens a text editor or image preview in a file tab.
  pub fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
    self.open(path, false, window, cx)
  }

  /// Opens a file in the preview tab (a single click in the Explorer): it is replaced by the next preview.
  pub fn open_preview(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
    self.open(path, true, window, cx)
  }

  /// Keeps the tab of `index` (a double click on it).
  fn pin_tab(&mut self, index: usize, cx: &mut Context<Self>) {
    if let Some(path) = self.tabs.items.get(index).cloned() {
      self.tabs.pin(&path);
      cx.notify();
    }
  }

  fn open(&mut self, path: PathBuf, preview: bool, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
    if let Some(format) = PreviewFormat::for_path(&path) {
      if !self.images.contains_key(&path) {
        self.load_image(path.clone(), format, cx);
      }
    } else {
      self.prepare(&path, window, cx)?;
      if let Some(file) = self.files.get_mut(&path) {
        file.painted = true;
      }
      self.touch(&path);
    }
    self.report_active = false;
    if preview {
      if let Some(replaced) = self.tabs.show_preview(path) {
        self.images.remove(&replaced);
        self.svg_source.remove(&replaced);
      }
    } else {
      self.tabs.show(path.clone(), |open| *open == path);
    }
    self.announce(cx);
    Ok(())
  }

  fn load_image(&mut self, path: PathBuf, format: PreviewFormat, cx: &mut Context<Self>) {
    let renderer = cx.svg_renderer();
    let source = self.files.get(&path).map(|file| file.state.read(cx).value().to_string());
    let input_path = path.clone();
    let work = cx.background_executor().spawn(async move { format.decode(&input_path, source, renderer) });
    let key = path.clone();
    let task = cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        let preview = match result {
          Ok(image) => ImagePreview::Ready(image),
          Err(error) => ImagePreview::Failed(error.into()),
        };
        this.images.insert(key, preview);
        cx.notify();
      });
    });
    self.images.insert(path, ImagePreview::Loading { _task: task });
  }

  fn show_svg_source(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) -> bool {
    if let Err(message) = self.prepare(path, window, cx) {
      window.push_notification(Notification::error(message), cx);
      return false;
    }
    self.svg_source.insert(path.to_path_buf());
    cx.notify();
    true
  }

  pub fn reveal_position(&mut self, path: &Path, line: usize, column: usize, window: &mut Window, cx: &mut Context<Self>) {
    if PreviewFormat::for_path(path) == Some(PreviewFormat::Svg) && !self.show_svg_source(path, window, cx) {
      return;
    }
    if let Some(file) = self.files.get(path) {
      file.state.update(cx, |state, cx| {
        state.set_cursor_position(gpui_kit::component::input::Position { line: line as u32, character: column as u32 }, window, cx);
      });
    }
  }

  /// Gets a file ready for opening while the pointer rests on it: reads it and draws its editor out of sight,
  /// which is when the highlighting happens, so the click only has to show it. Errors are ignored: opening the
  /// file for real reports them.
  pub fn prefetch(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
    if PreviewFormat::for_path(path).is_some() || fs::metadata(path).map_or(true, |metadata| metadata.len() > PREFETCH_MAX_BYTES) {
      return;
    }
    if self.prepare(path, window, cx).is_err() {
      return;
    }
    let Some(file) = self.files.get_mut(path).filter(|file| !file.painted) else { return };
    file.painted = true;
    self.preview = Some(file.state.clone());
    self.preview_epoch += 1;
    let epoch = self.preview_epoch;
    cx.notify();
    cx.spawn(async move |this, cx| {
      cx.background_executor().timer(PREVIEW_TIME).await;
      let _ = this.update(cx, |this, cx| {
        if this.preview_epoch == epoch {
          this.preview = None;
          cx.notify();
        }
      });
    })
    .detach();
  }

  fn prepare(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
    if self.files.contains_key(path) {
      return Ok(());
    }
    let metadata = fs::metadata(path).map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    if metadata.len() > MAX_FILE_BYTES {
      return Err(format!("{} is too large to open here.", display_name(path)));
    }
    let text = fs::read_to_string(path).map_err(|_| format!("{} is not a text file.", display_name(path)))?;
    self.install_text(path, text, window, cx);
    Ok(())
  }

  fn install_text(&mut self, path: &Path, text: String, window: &mut Window, cx: &mut Context<Self>) {
    let language = language_for(path);
    let crlf = text.contains("\r\n");
    let state = cx.new(|cx| EditorState::new(window, cx).language(language).line_number(true).default_value(text));
    let owner = path.to_path_buf();
    let changes = cx.subscribe(&state, move |this, _, event: &InputEvent, cx| {
      if matches!(event, InputEvent::Change) {
        if let Some(file) = this.files.get_mut(&owner) {
          file.dirty = true;
          this.tabs.pin(&owner);
          cx.notify();
        }
      }
    });
    let cursor = cx.observe(&state, |_, _, cx| cx.notify());
    self.files.insert(path.to_path_buf(), ReadyFile { state, dirty: false, painted: false, crlf, _changes: changes, _cursor: cursor });
    self.touch(path);
    self.evict();
  }

  pub fn lock_replacement_files(&mut self, paths: impl IntoIterator<Item = PathBuf>, cx: &mut Context<Self>) {
    self.replacement_files = paths.into_iter().collect();
    cx.notify();
  }

  pub fn update_replaced_text(&mut self, path: &Path, text: &str, window: &mut Window, cx: &mut Context<Self>) {
    if self.files.contains_key(path) && !self.is_dirty(path) {
      self.install_text(path, text.to_owned(), window, cx);
      cx.notify();
    }
    if let Some(format) = PreviewFormat::for_path(path).filter(|_| self.images.contains_key(path)) {
      self.load_image(path.to_path_buf(), format, cx);
    }
  }

  pub fn show_search_report(&mut self, query: &str, text: String, window: &mut Window, cx: &mut Context<Self>) {
    let title: SharedString = format!("Search: {query}").into();
    let editor = cx.new(|cx| EditorState::new(window, cx).default_value(text));
    self.search_report = Some((title, editor));
    self.report_active = true;
    self.announce(cx);
  }

  fn close_report(&mut self, cx: &mut Context<Self>) {
    self.search_report = None;
    self.report_active = false;
    self.announce(cx);
  }

  /// Marks a file as the most recently used.
  fn touch(&mut self, path: &Path) {
    self.recent.retain(|other| other != path);
    self.recent.push(path.to_path_buf());
  }

  /// Drops the least recently used editors beyond the limit, never the open file or one with unsaved changes.
  fn evict(&mut self) {
    // The files in tabs, in this project or another, stay.
    let tabs = std::iter::once(&self.tabs).chain(self.saved.values());
    let open: Vec<PathBuf> = tabs.flat_map(|tabs| tabs.items.iter()).cloned().collect();
    let mut index = 0;
    while self.files.len() > MAX_READY_FILES && index < self.recent.len() {
      let path = self.recent[index].clone();
      let keep = open.contains(&path) || self.files.get(&path).is_some_and(|file| file.dirty);
      if keep {
        index += 1;
      } else {
        self.files.remove(&path);
        self.recent.remove(index);
      }
    }
  }

  /// Drops what the editor remembers about a file whose changes were thrown away: its text (also unsaved edits), and
  /// its tab.
  pub fn forget(&mut self, path: &Path, cx: &mut Context<Self>) {
    self.files.remove(path);
    self.images.remove(path);
    self.svg_source.remove(path);
    for tabs in self.saved.values_mut() {
      if let Some(index) = tabs.items.iter().position(|open| open == path) {
        tabs.remove(index);
        tabs.close_order.reset();
      }
    }
    self.recent.retain(|recent| recent != path);
    self.tabs.pin(path);
    let kept: Vec<PathBuf> = self.tabs.items.iter().filter(|open| open.as_path() != path).cloned().collect();
    if kept.len() != self.tabs.items.len() {
      self.tabs.active = self.tabs.active.min(kept.len().saturating_sub(1));
      self.tabs.items = kept;
    }
    self.announce(cx);
  }

  /// Writes the open file to disk.
  pub fn save(&mut self, cx: &mut Context<Self>) -> io::Result<bool> {
    let Some(path) = self.tabs.current().filter(|_| !self.report_active) else { return Ok(false) };
    let Some(file) = self.files.get_mut(path) else { return Ok(false) };
    if self.replacement_files.contains(path) {
      return Err(io::Error::other("Wait for the project replacement to finish."));
    }
    fs::write(path, file.state.read(cx).value().to_string())?;
    file.dirty = false;
    cx.notify();
    Ok(true)
  }

  pub fn is_dirty(&self, path: &Path) -> bool {
    self.files.get(path).is_some_and(|file| file.dirty)
  }

  /// What the status bar says about the open file, left to right: the cursor, the indentation, the encoding, the line
  /// ending and the language.
  pub fn status(&self, cx: &App) -> Vec<String> {
    match self.tabs.current().filter(|_| !self.report_active) {
      None => Vec::new(),
      Some(path) => {
        if let Some(format) = PreviewFormat::for_path(path).filter(|_| !self.svg_source.contains(path)) {
          return vec![match format {
            PreviewFormat::Svg => "SVG preview",
            PreviewFormat::Icns => "ICNS preview",
          }
          .into()];
        }
        let Some(file) = self.files.get(path) else { return Vec::new() };
        let cursor = file.state.read(cx).cursor_position();
        vec![
          format!("Ln {}, Col {}", cursor.line + 1, cursor.character + 1),
          format!("Spaces: {INDENT_WIDTH}"),
          "UTF-8".into(),
          if file.crlf { "CRLF" } else { "LF" }.into(),
          language_label(path).into(),
        ]
      }
    }
  }
}

/// How many spaces one indent is: the editor's default.
const INDENT_WIDTH: usize = 2;

fn display_name(path: &Path) -> String {
  path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// The syntax highlighting language for a file, by its extension.
fn language_for(path: &Path) -> &'static str {
  match path.extension().and_then(|extension| extension.to_str()).unwrap_or_default() {
    "rs" => "rust",
    "toml" => "toml",
    "json" => "json",
    "md" | "markdown" => "markdown",
    "ts" | "tsx" => "typescript",
    "js" | "jsx" | "mjs" | "cjs" => "javascript",
    "html" | "htm" => "html",
    "css" => "css",
    "py" => "python",
    "sh" | "zsh" | "bash" => "bash",
    "yml" | "yaml" => "yaml",
    "swift" => "swift",
    "go" => "go",
    _ => "text",
  }
}

/// The name of a file's language as the status bar shows it.
fn language_label(path: &Path) -> &'static str {
  match language_for(path) {
    "rust" => "Rust",
    "toml" => "TOML",
    "json" => "JSON",
    "markdown" => "Markdown",
    "typescript" => "TypeScript",
    "javascript" => "JavaScript",
    "html" => "HTML",
    "css" => "CSS",
    "python" => "Python",
    "bash" => "Shell",
    "yaml" => "YAML",
    "swift" => "Swift",
    "go" => "Go",
    _ => "Plain Text",
  }
}

impl EventEmitter<PanelEvent> for EditorPanel {}

impl Focusable for EditorPanel {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus_handle.clone()
  }
}

impl BasePanel for EditorPanel {
  fn panel_name(&self) -> &'static str {
    "editor"
  }

  fn closable(&self, _: &App) -> bool {
    false
  }
}

impl EditorPanel {
  /// The tab bar, drawn by the panel itself so the first tab sits at the edge of the pane.
  fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
    let tabs = self.tabs.items.iter().enumerate().map(|(index, path)| {
      let name = format!("{}{}", display_name(path), if self.is_dirty(path) { " •" } else { "" });
      let label = div().italic().when(self.tabs.preview.as_ref() != Some(path), |label| label.not_italic()).child(name);
      let close = Button::new(SharedString::from(format!("close-tab:{}", path.display())))
        .ghost()
        .xsmall()
        .icon(Icon::empty().path("icons/x.svg").size_4())
        .tooltip("Close")
        .on_click(cx.listener(move |this, _, _, cx| {
          // The click must not also reach the tab and select it.
          cx.stop_propagation();
          this.close_tab(index, cx);
        }));
      strip_tab(SharedString::from(format!("editor-tab:{}", path.display())), label, !self.report_active && index == self.tabs.active, close, cx)
        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
          this.activate(index, cx);
          if event.click_count() >= 2 {
            this.pin_tab(index, cx);
          }
        }))
    });
    strip(cx).children(tabs).children(self.search_report.as_ref().map(|(title, _)| {
      let close = Button::new("close-search-report").ghost().xsmall().icon(IconName::Close).tooltip("Close search results").on_click(cx.listener(
        |this, _, _, cx| {
          cx.stop_propagation();
          this.close_report(cx);
        },
      ));
      strip_tab("search-report-tab".into(), title.clone(), self.report_active, close, cx).on_click(cx.listener(|this, _, _, cx| {
        this.report_active = true;
        this.announce(cx);
      }))
    }))
  }
}

impl Panel for EditorPanel {
  fn zoom_control(&self, _: &App) -> Option<PanelControl> {
    Some(PanelControl::Both)
  }

  fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    div().child("Editor")
  }

  /// The panel draws its own tabs (`render_tabs`).
  fn title_bar(&self, _: &App) -> bool {
    false
  }
}

impl Render for EditorPanel {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let content = if let Some((_, editor)) = self.search_report.as_ref().filter(|_| self.report_active) {
      Editor::new(editor).readonly(true).appearance(false).bordered(false).size_full().into_any_element()
    } else {
      match self.tabs.current() {
        None => Empty::new()
          .size_full()
          .header(
            EmptyHeader::new()
              .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(Icon::new(IconName::File)))
              .title(EmptyTitle::new().child("No file open"))
              .description(EmptyDescription::new().child("Choose a file in the Explorer.")),
          )
          .into_any_element(),
        Some(path) => {
          let show_source = self.svg_source.contains(path);
          let body = match self.images.get(path).filter(|_| !show_source) {
            Some(ImagePreview::Ready(image)) => {
              div().absolute().inset_4().child(img(image.clone()).size_full().min_w_0().min_h_0().object_fit(ObjectFit::Contain)).into_any_element()
            }
            Some(ImagePreview::Loading { .. }) => {
              Empty::new().size_full().header(EmptyHeader::new().title(EmptyTitle::new().child("Loading image…"))).into_any_element()
            }
            Some(ImagePreview::Failed(error)) => Empty::new()
              .size_full()
              .header(
                EmptyHeader::new()
                  .title(EmptyTitle::new().child("Image preview unavailable"))
                  .description(EmptyDescription::new().child(error.clone())),
              )
              .into_any_element(),
            None => self.files.get(path).map_or_else(
              || div().into_any_element(),
              |file| {
                div()
                  .size_full()
                  .cursor_text()
                  .child(
                    Editor::new(&file.state)
                      .readonly(self.read_only || self.replacement_files.contains(path))
                      .appearance(false)
                      .bordered(false)
                      .size_full(),
                  )
                  .into_any_element()
              },
            ),
          };
          if PreviewFormat::for_path(path) == Some(PreviewFormat::Svg) {
            let path = path.clone();
            v_flex()
              .size_full()
              .child(div().flex().justify_end().px_2().py_1().border_b_1().border_color(cx.theme().border).child(
                Button::new("toggle-svg-preview").ghost().small().label(if show_source { "Preview" } else { "Source" }).on_click(cx.listener(
                  move |this, _, window, cx| {
                    if this.svg_source.remove(&path) {
                      this.load_image(path.clone(), PreviewFormat::Svg, cx);
                      cx.notify();
                    } else {
                      this.show_svg_source(&path, window, cx);
                    }
                  },
                )),
              ))
              .child(div().relative().flex_1().min_h_0().child(body))
              .into_any_element()
          } else {
            body
          }
        }
      }
    };
    let hidden = self.preview.clone().or_else(|| self.warmup.clone());
    let tabs = (!self.tabs.items.is_empty() || self.search_report.is_some()).then(|| self.render_tabs(cx));
    v_flex().relative().size_full().children(tabs).child(div().relative().flex_1().min_h_0().child(content)).when_some(hidden, |this, state| {
      this.child(div().absolute().top_0().left_0().w(px(1.)).h(px(1.)).overflow_hidden().opacity(0.).child(Editor::new(&state)))
    })
  }
}
