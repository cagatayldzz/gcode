use super::*;
use gpui_kit::base::input::RopeExt as _;
use lsp_types::TextEdit;
use serde_json::json;

pub(crate) struct FormatResult {
  state: EntityId,
  source: SharedString,
  formatted: String,
  session: Arc<ws_language::Session>,
}

impl EditorPanel {
  pub(crate) fn request_format(&self, path: &Path, cx: &App) -> Result<Task<Result<FormatResult, String>>, String> {
    let file = self.files.get(path).ok_or("Open a text file to format it.")?;
    let session = self
      .session_for(path)
      .filter(|session| session.supports_formatting())
      .ok_or("No formatter is available. Enable a project language server that supports document formatting.")?;
    let source = file.state.read(cx).value();
    let state = file.state.entity_id();
    let preferences = self.editor_preferences(path);
    session.sync(path, &source);
    let reply = session.request(
      "textDocument/formatting",
      json!({
        "textDocument": { "uri": ws_language::file_uri(path) },
        "options": { "tabSize": preferences.tab_size, "insertSpaces": !preferences.hard_tabs }
      }),
    );
    Ok(cx.background_executor().spawn(async move {
      let result = reply.await.map_err(|_| "The formatter stopped or did not answer in time.")??;
      let edits: Option<Vec<TextEdit>> = serde_json::from_value(result).map_err(|error| error.to_string())?;
      let formatted = ws_language::apply_text_edits(&source, edits.as_deref().unwrap_or_default())?;
      Ok(FormatResult { state, source, formatted, session })
    }))
  }

  pub(crate) fn apply_format(&mut self, path: &Path, result: FormatResult, window: &mut Window, cx: &mut Context<Self>) -> bool {
    let Some(file) = self.files.get(path).filter(|file| file.state.entity_id() == result.state && !self.replacement_files.contains(path)) else {
      return false;
    };
    if file.state.read(cx).value() != result.source || !self.session_for(path).is_some_and(|session| Arc::ptr_eq(&session, &result.session)) {
      return false;
    }
    replace_document(&file.state, &result.source, result.formatted, window, cx);
    true
  }
}

pub(super) fn replace_document(state: &Entity<EditorState>, source: &str, replacement: String, window: &mut Window, cx: &mut App) {
  if replacement == source {
    return;
  }
  state.update(cx, |state, cx| {
    let mut cursor = state.cursor_position();
    state.replace_all(replacement, window, cx);
    cursor.line = cursor.line.min(state.text().lines_len().saturating_sub(1) as u32);
    state.set_cursor_position(cursor, window, cx);
  });
}
