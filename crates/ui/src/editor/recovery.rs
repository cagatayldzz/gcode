use super::*;
use std::sync::Mutex;
use ws_storage::{Draft, RecoveryJournal};

const CHECKPOINT_INTERVAL: Duration = Duration::from_millis(500);

/// Serializes checkpoints and removals. The UI only locks the queue long enough to submit new work.
pub(super) struct RecoveryWriter {
  journal: RecoveryJournal,
  pending: Mutex<HashMap<PathBuf, Option<Draft>>>,
  writing: Mutex<()>,
}

impl RecoveryWriter {
  pub(super) fn new(journal: RecoveryJournal) -> Self {
    Self { journal, pending: Mutex::default(), writing: Mutex::default() }
  }

  fn flush(&self) -> io::Result<()> {
    let _writing = self.writing.lock().unwrap();
    let batch = std::mem::take(&mut *self.pending.lock().unwrap());
    let mut failure = None;
    for (path, draft) in batch {
      let result = match &draft {
        Some(draft) => self.journal.store(draft),
        None => self.journal.remove(&path),
      };
      if let Err(error) = result {
        failure = Some(error);
        self.pending.lock().unwrap().entry(path).or_insert(draft);
      }
    }
    failure.map_or(Ok(()), Err)
  }
}

impl EditorPanel {
  pub(crate) fn start_recovery(
    &mut self,
    profile: &ws_storage::Profile,
    projects: &ws_storage::ProjectList,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let journal = RecoveryJournal::new(&profile.dir());
    let writer = Arc::new(RecoveryWriter::new(journal));
    self.recovery = Some(writer.clone());
    self.recovery_loading = true;
    let roots: Vec<_> = projects.projects.iter().map(|project| project.path.clone()).collect();
    self.set_root(projects.active.clone(), cx);
    self._recovery_task = Some(cx.spawn_in(window, async move |this, cx| {
      let reader = writer.clone();
      let loaded = cx
        .background_executor()
        .spawn(async move {
          let (mut drafts, unreadable) = reader.journal.load()?;
          drafts.retain(|draft| {
            let path = draft.path();
            let saved = FileVersion::read(&path).ok().flatten().is_some_and(|version| version == version.with_contents(draft.text.as_bytes()));
            !saved || reader.journal.remove(&path).is_err()
          });
          Ok::<_, io::Error>((drafts, unreadable))
        })
        .await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.recovery_loading = false;
        match loaded {
          Ok((drafts, unreadable)) => {
            let restored = this.restore_drafts(drafts, &roots, window, cx);
            if restored > 0 {
              window.push_notification(
                gpui_kit::component::notification::Notification::info(format!("Recovered unsaved edits in {restored} file(s).")),
                cx,
              );
            }
            if unreadable > 0 {
              window.push_notification(
                crate::failure(format!("Could not read {unreadable} recovery file(s). They remain in the profile’s recovery folder.")),
                cx,
              );
            }
          }
          Err(error) => window.push_notification(crate::failure(format!("Could not load editor recovery: {error}")), cx),
        }
      });
    }));
    self.start_checkpointing(window, cx);
  }

  pub(crate) fn share_recovery(&mut self, primary: &Entity<EditorPanel>, window: &mut Window, cx: &mut Context<Self>) {
    let primary = primary.read(cx);
    self.recovery = primary.recovery.clone();
    self.replacement_files.clone_from(&primary.replacement_files);
    self.start_checkpointing(window, cx);
  }

  fn start_checkpointing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(writer) = self.recovery.clone() else { return };
    self._checkpoint_task = Some(cx.spawn_in(window, async move |this, cx| {
      let mut reported = false;
      loop {
        cx.background_executor().timer(CHECKPOINT_INTERVAL).await;
        match this.update(cx, |this, cx| this.queue_recovery(cx)) {
          Ok(true) => {}
          Ok(false) => continue,
          Err(_) => break,
        };
        let writer = writer.clone();
        let result = cx.background_executor().spawn(async move { writer.flush() }).await;
        match result {
          Ok(()) => reported = false,
          Err(error) if !reported => {
            let _ = this.update_in(cx, |_, window, cx| {
              window.push_notification(crate::failure(format!("Could not checkpoint unsaved edits: {error}. Save your work before closing.")), cx);
            });
            reported = true;
          }
          Err(_) => {}
        }
      }
    }));
  }

  fn restore_drafts(&mut self, drafts: Vec<Draft>, roots: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) -> usize {
    let shown = self.active_file().map(Path::to_path_buf);
    let mut restored = 0;
    for draft in drafts {
      let path = draft.path();
      if self.is_dirty(&path) || self.recovery_pending.contains(&path) {
        continue;
      }
      self.install_text(&path, draft.text, draft.base, window, cx);
      if let Some(file) = self.files.get_mut(&path) {
        file.dirty = true;
        file.painted = true;
      }
      if PreviewFormat::for_path(&path).is_some() {
        self.svg_source.insert(path.clone());
      }
      let project = roots.iter().filter(|root| path.starts_with(root)).max_by_key(|root| root.components().count());
      let tabs = if let Some(project) = project.filter(|root| self.root.as_ref() != Some(*root)) {
        self.saved.entry(project.clone()).or_default()
      } else {
        &mut self.tabs
      };
      tabs.show(path.clone(), |open| open == &path);
      restored += 1;
    }
    if restored > 0 {
      if let Some(index) = shown.and_then(|shown| self.tabs.items.iter().position(|path| path == &shown)) {
        self.tabs.active = index;
      }
      self.announce(cx);
    }
    restored
  }

  fn queue_recovery(&mut self, cx: &App) -> bool {
    let Some(writer) = &self.recovery else { return false };
    if self.recovery_pending.is_empty() {
      return !writer.pending.lock().unwrap().is_empty();
    }
    let batch = self
      .recovery_pending
      .drain()
      .map(|path| {
        let draft =
          self.files.get(&path).filter(|file| file.dirty).map(|file| Draft::new(&path, file.state.read(cx).value().to_string(), file.disk.clone()));
        (path, draft)
      })
      .collect::<Vec<_>>();
    writer.pending.lock().unwrap().extend(batch);
    true
  }

  pub(crate) fn flush_recovery(&mut self, cx: &App) -> io::Result<()> {
    self.queue_recovery(cx);
    self.recovery.as_ref().map_or(Ok(()), |writer| writer.flush())
  }

  pub(super) fn remove_recovery_for(&mut self, path: &Path, cx: &App) {
    if self.recovery.is_some() {
      self.recovery_pending.insert(path.to_path_buf());
      self.queue_recovery(cx);
    }
  }
}
