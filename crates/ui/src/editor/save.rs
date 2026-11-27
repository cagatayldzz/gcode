use super::*;

pub(crate) enum SaveOutcome {
  Saved(FileVersion),
  Conflict(Option<FileVersion>),
}

pub(crate) struct SaveRequest {
  pub state: EntityId,
  pub text: SharedString,
  expected: Option<FileVersion>,
  endings: ws_storage::LineEndings,
}

impl SaveRequest {
  pub fn write(&self, path: &Path) -> io::Result<SaveOutcome> {
    let current = FileVersion::read(path)?;
    if current != self.expected {
      return Ok(SaveOutcome::Conflict(current));
    }
    let text = self.endings.encode(&self.text);
    if !ws_storage::write_if_current(path, text.as_bytes(), current.as_ref())? {
      return Ok(SaveOutcome::Conflict(FileVersion::read(path)?));
    }
    let version = match current {
      Some(version) => version.with_contents(text.as_bytes()),
      None => FileVersion::from_contents(path, text.as_bytes()),
    };
    Ok(SaveOutcome::Saved(version))
  }
}

impl EditorPanel {
  pub(crate) fn prepare_save(
    &mut self,
    path: &Path,
    approval: Option<&Option<FileVersion>>,
    cx: &mut Context<Self>,
  ) -> io::Result<Option<SaveRequest>> {
    self.tabs.keep_open(path);
    cx.notify();
    let Some(file) = self.files.get(path) else { return Ok(None) };
    if self.replacement_files.contains(path) {
      return Err(io::Error::other("Wait for the project replacement to finish."));
    }
    Ok(Some(SaveRequest {
      state: file.state.entity_id(),
      text: file.state.read(cx).value(),
      expected: approval.unwrap_or(&file.disk).clone(),
      endings: self.editor_preferences(path).line_endings,
    }))
  }

  pub(crate) fn finish_save(&mut self, path: &Path, request: SaveRequest, version: FileVersion, cx: &mut Context<Self>) {
    let Some(file) = self.files.get_mut(path).filter(|file| file.state.entity_id() == request.state) else { return };
    file.disk = Some(version);
    file.crlf = request.endings.crlf(request.text.contains("\r\n"));
    file.dirty = file.state.read(cx).value() != request.text;
    self.remove_recovery_for(path, cx);
    cx.notify();
  }

  pub(crate) fn buffer_text(&self, path: &Path, cx: &App) -> Option<String> {
    self.files.get(path).map(|file| file.state.read(cx).value().to_string())
  }

  pub(crate) fn reload_from_disk(
    &mut self,
    path: &Path,
    version: Option<&FileVersion>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> io::Result<bool> {
    if self.replacement_files.contains(path) {
      return Err(io::Error::other("Wait for the project replacement to finish."));
    }
    if let Some(version) = version {
      let (text, current) = read_editor_text(path)?;
      if &current != version {
        return Ok(false);
      }
      self.install_text(path, text, Some(current), window, cx);
      if let Some(file) = self.files.get_mut(path) {
        file.painted = true;
      }
    } else {
      if FileVersion::read(path)?.is_some() {
        return Ok(false);
      }
      self.forget(path, cx);
    }
    self.remove_recovery_for(path, cx);
    cx.notify();
    Ok(true)
  }

  pub(crate) fn save_copy(&mut self, path: &Path, destination: &Path, cx: &mut Context<Self>) -> io::Result<bool> {
    let Some(text) = self.buffer_text(path, cx) else { return Ok(false) };
    if destination != path && self.is_dirty(destination) {
      return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Another editor has unsaved changes for that destination."));
    }
    if self.replacement_files.contains(path) || self.replacement_files.contains(destination) {
      return Err(io::Error::other("Wait for the project replacement to finish."));
    }
    let text = self.editor_preferences(destination).line_endings.encode(&text);
    ws_storage::write_new(destination, text.as_bytes())?;
    self.forget(path, cx);
    self.remove_recovery_for(path, cx);
    Ok(true)
  }
}
