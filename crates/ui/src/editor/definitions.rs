//! Connects the editor’s definition, completion and hover providers to the approved language server.

use super::*;
use anyhow::Result;
use gpui_kit::base::input::{CompletionProvider, DefinitionProvider, HoverProvider, Position, RopeExt as _};
use lsp_types::{CompletionContext, CompletionResponse, CompletionTextEdit, Hover, LocationLink, Range, Uri};
use serde_json::{json, Value};
use std::str::FromStr as _;
use ws_language::Location;

/// A place the editor should show, sent when a definition lies in another file.
pub(crate) struct OpenLocation {
  pub path: PathBuf,
  pub start: Position,
  pub end: Position,
}

impl EventEmitter<OpenLocation> for EditorPanel {}

/// Asks the language server of the file's project where a name is defined.
struct LanguageProvider {
  pub panel: WeakEntity<EditorPanel>,
  pub path: PathBuf,
}

impl LanguageProvider {
  fn query(&self, method: &str, text: &Rope, offset: usize, context: Option<Value>, cx: &App) -> Option<futures::channel::oneshot::Receiver<Value>> {
    let session = self.panel.read_with(cx, |panel, _| panel.session_for(&self.path)).ok().flatten()?;
    session.sync(&self.path, &text.to_string());
    let point = text.offset_to_point(offset);
    let line = text.slice_line(point.row);
    let position = Position::new(point.row as u32, line.byte_to_utf16_idx(point.column) as u32);
    let mut params = json!({ "textDocument": { "uri": ws_language::file_uri(&self.path) }, "position": position });
    if let Some(context) = context {
      params["context"] = context;
    }
    Some(session.request(method, params))
  }
}

impl DefinitionProvider for LanguageProvider {
  fn definitions(&self, text: &Rope, offset: usize, _: &mut Window, cx: &mut App) -> Task<Result<Vec<LocationLink>>> {
    let Some(answer) = self.query("textDocument/definition", text, offset, None, cx) else { return Task::ready(Ok(Vec::new())) };
    let text = text.clone();
    let path = self.path.clone();
    cx.background_executor().spawn(async move {
      let results = ws_language::parse_locations(&answer.await.unwrap_or_default());
      Ok(
        results
          .iter()
          .filter_map(|result| {
            let mut link = link(result)?;
            link.origin_selection_range = link.origin_selection_range.map(|range| editor_range(&text, range));
            if result.path == path {
              link.target_selection_range = editor_range(&text, link.target_selection_range);
              link.target_range = link.target_selection_range;
            }
            Some(link)
          })
          .collect(),
      )
    })
  }
}

impl CompletionProvider for LanguageProvider {
  fn completions(&self, text: &Rope, offset: usize, trigger: CompletionContext, _: &mut Window, cx: &mut App) -> Task<Result<CompletionResponse>> {
    let Some(answer) = self.query("textDocument/completion", text, offset, serde_json::to_value(trigger).ok(), cx) else {
      return Task::ready(Ok(CompletionResponse::Array(Vec::new())));
    };
    let text = text.clone();
    cx.background_executor().spawn(async move {
      let reply: Value = answer.await.unwrap_or_default();
      let mut completions = serde_json::from_value(reply).unwrap_or_else(|_| CompletionResponse::Array(Vec::new()));
      let items = match &mut completions {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => &mut list.items,
      };
      // The Kit applies one plain-text edit per completion. Avoid presenting an item whose other edits would be lost.
      items.retain(|item| {
        item.additional_text_edits.as_ref().is_none_or(Vec::is_empty)
          && item.insert_text_format != Some(lsp_types::InsertTextFormat::SNIPPET)
          && item.command.is_none()
      });
      let prefix_bytes: usize = text
        .chars_at(offset)
        .reversed()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '$'))
        .map(char::len_utf8)
        .sum();
      let fallback = Range::new(text.offset_to_position(offset - prefix_bytes), text.offset_to_position(offset));
      for item in items {
        if let Some(edit) = &mut item.text_edit {
          match edit {
            CompletionTextEdit::Edit(edit) => edit.range = editor_range(&text, edit.range),
            CompletionTextEdit::InsertAndReplace(edit) => {
              edit.insert = editor_range(&text, edit.insert);
              edit.replace = editor_range(&text, edit.replace);
            }
          }
        } else if let Some(new_text) = item.insert_text.take() {
          item.text_edit = Some(CompletionTextEdit::Edit(lsp_types::TextEdit { range: fallback, new_text }));
        }
      }
      Ok(completions)
    })
  }

  fn is_completion_trigger(&self, _: usize, new_text: &str, cx: &mut App) -> bool {
    new_text
      .chars()
      .next()
      .is_some_and(|character| new_text.chars().count() == 1 && (character.is_alphanumeric() || matches!(character, '.' | '_' | '$' | '<' | ' ')))
      && self.panel.read_with(cx, |panel, _| panel.session_for(&self.path).is_some()).unwrap_or(false)
  }
}

impl HoverProvider for LanguageProvider {
  fn hover(&self, text: &Rope, offset: usize, _: &mut Window, cx: &mut App) -> Task<Result<Option<Hover>>> {
    let Some(answer) = self.query("textDocument/hover", text, offset, None, cx) else { return Task::ready(Ok(None)) };
    let text = text.clone();
    cx.background_executor().spawn(async move {
      let mut hover: Option<Hover> = serde_json::from_value(answer.await.unwrap_or_default()).ok().flatten();
      if let Some(hover) = &mut hover {
        hover.range = hover.range.map(|range| editor_range(&text, range));
      }
      Ok(hover)
    })
  }
}

pub(super) fn editor_position(text: &Rope, position: Position) -> Position {
  let row = (position.line as usize).min(text.lines_len().saturating_sub(1));
  let line = text.slice_line(row);
  let byte = line.utf16_to_byte_idx((position.character as usize).min(line.byte_to_utf16_idx(line.len())));
  Position::new(row as u32, line.byte_to_char_idx(byte) as u32)
}

fn editor_range(text: &Rope, range: Range) -> Range {
  Range::new(editor_position(text, range.start), editor_position(text, range.end))
}

fn link(location: &Location) -> Option<LocationLink> {
  let range = |start: (u32, u32), end: (u32, u32)| Range {
    start: Position { line: start.0, character: start.1 },
    end: Position { line: end.0, character: end.1 },
  };
  let target = range((location.line, location.column), (location.end_line, location.end_column));
  Some(LocationLink {
    origin_selection_range: location.origin.map(|(line, column, end_line, end_column)| range((line, column), (end_line, end_column))),
    target_uri: Uri::from_str(&ws_language::file_uri(&location.path)).ok()?,
    target_range: target,
    target_selection_range: target,
  })
}

impl EditorPanel {
  /// Makes ⌘-click work in the editor of `path`: the provider asks the language server, and a definition in another file is
  /// sent out as `OpenLocation` for the workspace to open (the same file is handled by the editor itself).
  pub(super) fn enable_language_features(&self, path: &Path, state: &Entity<EditorState>, cx: &mut Context<Self>) {
    if self.read_only || ws_language::language_id(path).is_none() {
      return;
    }
    let panel = cx.weak_entity();
    let owner = path.to_path_buf();
    state.update(cx, |state, cx| {
      let lsp = state.lsp_mut();
      let provider = Rc::new(LanguageProvider { panel: panel.clone(), path: owner.clone() });
      lsp.definition_provider = Some(provider.clone());
      lsp.completion_provider = Some(provider.clone());
      lsp.hover_provider = Some(provider);
      lsp.show_document = Some(Rc::new(move |params, window, cx| {
        let Some(target) = ws_language::file_path(params.uri.as_str()).filter(|target| *target != owner) else { return false };
        let selection = params.selection.unwrap_or_default();
        let panel = panel.clone();
        // The editor is being updated right now: the rest waits until it is done.
        window.defer(cx, move |_, cx| {
          let _ = panel.update(cx, |_, cx| cx.emit(OpenLocation { path: target, start: selection.start, end: selection.end }));
        });
        true
      }));
      state.refresh(cx);
    });
  }

  /// Allows a file over the usual size limit to open, because a definition led there.
  pub fn allow_large(&mut self, path: &Path) {
    self.large_ok.insert(path.to_path_buf());
  }

  /// Selects a range of a file's text and shows it.
  pub fn reveal_range(&mut self, path: &Path, start: Position, end: Position, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(file) = self.files.get(path) {
      file.state.update(cx, |state, cx| {
        let (start, end) = (editor_position(state.text(), start), editor_position(state.text(), end));
        let (from, to) = (state.text().position_to_offset(&start), state.text().position_to_offset(&end));
        state.set_cursor_position(start, window, cx);
        state.set_selected_range(from..to, cx);
      });
    }
  }
}
