//! The editor area: a code editor for a file, or the diff of a changed file.

use gpui_kit::{
  component::{
    diff::{Diff, DiffFile, DiffMode, DiffState},
    dock::{BasePanel, Panel, PanelEvent},
    empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle},
    input::{Editor, EditorState, InputEvent},
    Icon, IconName,
  },
  *,
};
use std::{fs, io, path::Path, path::PathBuf};

/// Files larger than this are not opened: the editor reads the whole text.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

enum Content {
  Nothing,
  File { path: PathBuf, state: Entity<EditorState>, dirty: bool, _changes: Subscription },
  Diff { name: String, state: Entity<DiffState> },
}

pub(crate) struct EditorPanel {
  content: Content,
  focus_handle: FocusHandle,
}

impl EditorPanel {
  pub fn new(cx: &mut Context<Self>) -> Self {
    Self { content: Content::Nothing, focus_handle: cx.focus_handle() }
  }

  /// Opens a text file for editing.
  pub fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
    let metadata = fs::metadata(&path).map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    if metadata.len() > MAX_FILE_BYTES {
      return Err(format!("{} is too large to open here.", display_name(&path)));
    }
    let text = fs::read_to_string(&path).map_err(|_| format!("{} is not a text file.", display_name(&path)))?;
    let language = language_for(&path);
    let state = cx.new(|cx| EditorState::new(window, cx).language(language).line_number(true).default_value(text));
    let changes = cx.subscribe(&state, |this, _, event: &InputEvent, cx| {
      if matches!(event, InputEvent::Change) {
        if let Content::File { dirty, .. } = &mut this.content {
          *dirty = true;
          cx.notify();
        }
      }
    });
    self.content = Content::File { path, state, dirty: false, _changes: changes };
    cx.notify();
    Ok(())
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
    let Content::File { path, state, dirty, .. } = &mut self.content else { return Ok(false) };
    fs::write(&*path, state.read(cx).value().to_string())?;
    *dirty = false;
    cx.notify();
    Ok(true)
  }

  /// What the status bar says about the open file: its name and language.
  pub fn status(&self) -> Option<String> {
    match &self.content {
      Content::Nothing => None,
      Content::File { path, dirty, .. } => Some(format!("{}{} · {}", display_name(path), if *dirty { " •" } else { "" }, language_for(path))),
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
  fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    match &self.content {
      Content::Nothing => "Editor".to_string(),
      Content::File { path, dirty, .. } => format!("{}{}", display_name(path), if *dirty { " •" } else { "" }),
      Content::Diff { name, .. } => format!("{name} (changes)"),
    }
  }
}

impl Render for EditorPanel {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    match &self.content {
      Content::Nothing => Empty::new()
        .size_full()
        .header(
          EmptyHeader::new()
            .media(EmptyMedia::new().with_variant(EmptyMediaVariant::Icon).child(Icon::new(IconName::File)))
            .title(EmptyTitle::new().child("No file open"))
            .description(EmptyDescription::new().child("Choose a file in the Explorer, or a changed file in Git.")),
        )
        .into_any_element(),
      Content::File { state, .. } => Editor::new(state).appearance(false).bordered(false).size_full().into_any_element(),
      Content::Diff { state, .. } => Diff::new(state).size_full().into_any_element(),
    }
  }
}
