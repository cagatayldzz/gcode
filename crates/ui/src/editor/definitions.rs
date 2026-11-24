//! Go to definition: ⌘-click on a name (or on an import) opens the place where it is defined, asked of the language server.

use super::*;
use anyhow::Result;
use gpui_kit::base::input::{DefinitionProvider, Position, RopeExt as _};
use lsp_types::{LocationLink, Range, Uri};
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
pub(super) struct Definitions {
  pub panel: WeakEntity<EditorPanel>,
  pub path: PathBuf,
}

impl DefinitionProvider for Definitions {
  fn definitions(&self, text: &Rope, offset: usize, _: &mut Window, cx: &mut App) -> Task<Result<Vec<LocationLink>>> {
    let session = self.panel.read_with(cx, |panel, _| panel.session_for(&self.path)).ok().flatten();
    let Some(session) = session else { return Task::ready(Ok(Vec::new())) };
    let position = text.offset_to_position(offset);
    let answer = session.definition(&self.path, position.line, position.character);
    cx.background_executor().spawn(async move { Ok(answer.await.unwrap_or_default().iter().filter_map(link).collect()) })
  }
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
  pub(super) fn enable_definitions(&self, path: &Path, state: &Entity<EditorState>, cx: &mut Context<Self>) {
    if self.read_only || ws_language::language_id(path).is_none() {
      return;
    }
    let panel = cx.weak_entity();
    let owner = path.to_path_buf();
    state.update(cx, |state, _| {
      let lsp = state.lsp_mut();
      lsp.definition_provider = Some(Rc::new(Definitions { panel: panel.clone(), path: owner.clone() }));
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
        let (from, to) = (state.text().position_to_offset(&start), state.text().position_to_offset(&end));
        state.set_cursor_position(start, window, cx);
        state.set_selected_range(from..to, cx);
      });
    }
  }
}
