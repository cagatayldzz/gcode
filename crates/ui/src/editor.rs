//! The editor area: a code editor for a file, or the diff of a changed file.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    diff::{Diff, DiffFile, DiffMode, DiffState},
    dock::{BasePanel, Panel, PanelControl, PanelEvent},
    empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle},
    input::{Editor, EditorState, InputEvent},
    Icon, IconName,
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
  _changes: Subscription,
}

enum Content {
  Nothing,
  File(PathBuf),
  Diff { name: String, state: Entity<DiffState> },
}

pub(crate) struct EditorPanel {
  content: Content,
  /// Editors that are ready, by path. Reopening one is instant and keeps its unsaved text.
  files: HashMap<PathBuf, ReadyFile>,
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
      content: Content::Nothing,
      files: HashMap::new(),
      recent: Vec::new(),
      warmup: None,
      _warmup_task: warmup_task,
      preview: None,
      preview_epoch: 0,
      focus_handle: cx.focus_handle(),
    }
  }

  /// Opens a text file for editing, at once if its editor is already ready.
  pub fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
    self.prepare(&path, window, cx)?;
    if let Some(file) = self.files.get_mut(&path) {
      file.painted = true;
    }
    self.touch(&path);
    self.content = Content::File(path);
    cx.notify();
    Ok(())
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
    self.files.insert(path.to_path_buf(), ReadyFile { state, dirty: false, painted: false, _changes: changes });
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
    let open = match &self.content {
      Content::File(path) => Some(path.clone()),
      _ => None,
    };
    let mut index = 0;
    while self.files.len() > MAX_READY_FILES && index < self.recent.len() {
      let path = self.recent[index].clone();
      let keep = Some(&path) == open.as_ref() || self.files.get(&path).is_some_and(|file| file.dirty);
      if keep {
        index += 1;
      } else {
        self.files.remove(&path);
        self.recent.remove(index);
      }
    }
  }

  /// Shows the changes of one file, given as a unified diff.
  pub fn open_diff(&mut self, name: String, patch: &str, cx: &mut Context<Self>) -> Result<(), String> {
    let files = DiffFile::parse(patch).map_err(|error| format!("Could not read the changes of {name}: {error}"))?;
    if files.is_empty() {
      return Err(format!("{name} has no text changes to show."));
    }
    let state = cx.new(|cx| DiffState::new(files, cx).with_mode(DiffMode::Unified));
    self.content = Content::Diff { name, state };
    cx.notify();
    Ok(())
  }

  /// Writes the open file to disk.
  pub fn save(&mut self, cx: &mut Context<Self>) -> io::Result<bool> {
    let Content::File(path) = &self.content else { return Ok(false) };
    let Some(file) = self.files.get_mut(path) else { return Ok(false) };
    fs::write(path, file.state.read(cx).value().to_string())?;
    file.dirty = false;
    cx.notify();
    Ok(true)
  }

  fn is_dirty(&self, path: &Path) -> bool {
    self.files.get(path).is_some_and(|file| file.dirty)
  }

  /// What the status bar says about the open file: its name and language.
  pub fn status(&self) -> Option<String> {
    match &self.content {
      Content::Nothing => None,
      Content::File(path) => Some(format!("{}{} · {}", display_name(path), if self.is_dirty(path) { " •" } else { "" }, language_for(path))),
      Content::Diff { name, .. } => Some(format!("{name} · changes")),
    }
  }
}

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

impl Panel for EditorPanel {
  fn zoom_control(&self, _: &App) -> Option<PanelControl> {
    Some(PanelControl::Both)
  }

  fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    match &self.content {
      Content::Nothing => "Editor".to_string(),
      Content::File(path) => format!("{}{}", display_name(path), if self.is_dirty(path) { " •" } else { "" }),
      Content::Diff { name, .. } => format!("{name} (changes)"),
    }
  }
}

impl Render for EditorPanel {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    let content = match &self.content {
      Content::Nothing => Empty::new()
        .size_full()
        .header(
          EmptyHeader::new()
            .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(Icon::new(IconName::File)))
            .title(EmptyTitle::new().child("No file open"))
            .description(EmptyDescription::new().child("Choose a file in the Explorer, or a changed file in Git.")),
        )
        .into_any_element(),
      Content::File(path) => match self.files.get(path) {
        Some(file) => Editor::new(&file.state).appearance(false).bordered(false).size_full().into_any_element(),
        None => div().into_any_element(),
      },
      Content::Diff { state, .. } => Diff::new(state).size_full().into_any_element(),
    };
    let hidden = self.preview.clone().or_else(|| self.warmup.clone());
    div().relative().size_full().child(content).when_some(hidden, |this, state| {
      this.child(div().absolute().top_0().left_0().w(px(1.)).h(px(1.)).overflow_hidden().opacity(0.).child(Editor::new(&state)))
    })
  }
}
