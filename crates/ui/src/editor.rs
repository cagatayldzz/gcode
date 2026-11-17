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
    v_flex, Icon, IconName, Sizable as _,
  },
  *,
};
use std::{collections::HashMap, fs, io, path::Path, path::PathBuf, time::Duration};

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

/// The tabs of a project and the one that is shown.
#[derive(Default)]
struct Tabs {
  /// The files in the tabs, in tab order.
  items: Vec<PathBuf>,
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

  /// Takes away a tab; the one on its right (else on its left) is shown.
  fn remove(&mut self, index: usize) {
    if index >= self.items.len() {
      return;
    }
    self.items.remove(index);
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
  /// Editors that are ready, by path. Reopening one is instant and keeps its unsaved text.
  files: HashMap<PathBuf, ReadyFile>,
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
}

impl EditorPanel {
  pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
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
      files: HashMap::new(),
      saved: HashMap::new(),
      recent: Vec::new(),
      warmup: None,
      _warmup_task: warmup_task,
      preview: None,
      preview_epoch: 0,
      focus_handle: cx.focus_handle(),
    }
  }

  /// Switches to another project's editor area: the tabs of the project being left are put away and come back with
  /// it; a project not seen before starts with none.
  pub fn switch_project(&mut self, from: Option<PathBuf>, to: Option<PathBuf>, cx: &mut Context<Self>) {
    let current = std::mem::take(&mut self.tabs);
    if let Some(from) = from {
      self.saved.insert(from, current);
    }
    if let Some(tabs) = to.and_then(|to| self.saved.remove(&to)) {
      self.tabs = tabs;
    }
    self.announce(cx);
  }

  /// Tells the others what is shown and redraws.
  fn announce(&mut self, cx: &mut Context<Self>) {
    let shown = match self.tabs.current() {
      Some(path) => ActiveTab::File(path.clone()),
      None => ActiveTab::Nothing,
    };
    cx.emit(shown);
    cx.notify();
  }

  /// Shows another tab.
  fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
    if index < self.tabs.items.len() {
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
    let Some(index) = self.tabs.close_order.next(self.tabs.items.len(), self.tabs.active) else { return false };
    self.close_tab(index, cx);
    true
  }

  /// Closes a tab. A file with unsaved text keeps it in memory: opening the file again brings it back.
  fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
    self.tabs.remove(index);
    self.announce(cx);
  }

  /// Opens a text file for editing, at once if its editor is already ready.
  pub fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
    self.prepare(&path, window, cx)?;
    if let Some(file) = self.files.get_mut(&path) {
      file.painted = true;
    }
    self.touch(&path);
    self.tabs.show(path.clone(), |open| *open == path);
    self.announce(cx);
    Ok(())
  }

  pub fn reveal_position(&mut self, path: &Path, line: usize, column: usize, window: &mut Window, cx: &mut Context<Self>) {
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
    if fs::metadata(path).map_or(true, |metadata| metadata.len() > PREFETCH_MAX_BYTES) {
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
    let language = language_for(path);
    let crlf = text.contains("\r\n");
    let state = cx.new(|cx| EditorState::new(window, cx).language(language).line_number(true).default_value(text));
    let owner = path.to_path_buf();
    let changes = cx.subscribe(&state, move |this, _, event: &InputEvent, cx| {
      if matches!(event, InputEvent::Change) {
        if let Some(file) = this.files.get_mut(&owner) {
          file.dirty = true;
          cx.notify();
        }
      }
    });
    let cursor = cx.observe(&state, |_, _, cx| cx.notify());
    self.files.insert(path.to_path_buf(), ReadyFile { state, dirty: false, painted: false, crlf, _changes: changes, _cursor: cursor });
    self.touch(path);
    self.evict();
    Ok(())
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
    self.recent.retain(|recent| recent != path);
    let kept: Vec<PathBuf> = self.tabs.items.iter().filter(|open| open.as_path() != path).cloned().collect();
    if kept.len() != self.tabs.items.len() {
      self.tabs.active = self.tabs.active.min(kept.len().saturating_sub(1));
      self.tabs.items = kept;
    }
    self.announce(cx);
  }

  /// Writes the open file to disk.
  pub fn save(&mut self, cx: &mut Context<Self>) -> io::Result<bool> {
    let Some(path) = self.tabs.current() else { return Ok(false) };
    let Some(file) = self.files.get_mut(path) else { return Ok(false) };
    fs::write(path, file.state.read(cx).value().to_string())?;
    file.dirty = false;
    cx.notify();
    Ok(true)
  }

  fn is_dirty(&self, path: &Path) -> bool {
    self.files.get(path).is_some_and(|file| file.dirty)
  }

  /// What the status bar says about the open file, left to right: the cursor, the indentation, the encoding, the line
  /// ending and the language.
  pub fn status(&self, cx: &App) -> Vec<String> {
    match self.tabs.current() {
      None => Vec::new(),
      Some(path) => {
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
      let label = format!("{}{}", display_name(path), if self.is_dirty(path) { " •" } else { "" });
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
      strip_tab(SharedString::from(format!("editor-tab:{}", path.display())), label, index == self.tabs.active, close, cx)
        .on_click(cx.listener(move |this, _, _, cx| this.activate(index, cx)))
    });
    strip(cx).children(tabs)
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
    let content = match self.tabs.current() {
      None => Empty::new()
        .size_full()
        .header(
          EmptyHeader::new()
            .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(Icon::new(IconName::File)))
            .title(EmptyTitle::new().child("No file open"))
            .description(EmptyDescription::new().child("Choose a file in the Explorer.")),
        )
        .into_any_element(),
      Some(path) => match self.files.get(path) {
        Some(file) => Editor::new(&file.state).appearance(false).bordered(false).size_full().into_any_element(),
        None => div().into_any_element(),
      },
    };
    let hidden = self.preview.clone().or_else(|| self.warmup.clone());
    let tabs = (!self.tabs.items.is_empty()).then(|| self.render_tabs(cx));
    v_flex().relative().size_full().children(tabs).child(div().flex_1().min_h_0().child(content)).when_some(hidden, |this, state| {
      this.child(div().absolute().top_0().left_0().w(px(1.)).h(px(1.)).overflow_hidden().opacity(0.).child(Editor::new(&state)))
    })
  }
}
