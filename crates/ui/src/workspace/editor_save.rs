use super::*;
use crate::{
  editor::{read_editor_text, SaveOutcome},
  SaveFile,
};
use gpui_kit::component::{
  dialog::DialogFooter,
  diff::{Diff, DiffFile, DiffState},
};
use ws_storage::FileVersion;

impl Workspace {
  pub fn save_file(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
    if self.file_actions.focused_side(window, cx).is_some() {
      return;
    }
    if let Some(path) = self.editor_panel.read(cx).active_file().map(Path::to_path_buf) {
      self.save_editor_file(path, None, window, cx);
    }
  }

  fn save_editor_file(&mut self, path: PathBuf, approval: Option<&Option<FileVersion>>, window: &mut Window, cx: &mut Context<Self>) {
    if self.editor_saves.contains_key(&path) {
      return;
    }
    let request = match self.editor_panel.update(cx, |editor, cx| editor.prepare_save(&path, approval, cx)) {
      Ok(Some(request)) => request,
      Ok(None) => return,
      Err(error) => {
        window.push_notification(crate::failure(format!("Could not save: {error}")), cx);
        return;
      }
    };
    let target = path.clone();
    let work = cx.background_executor().spawn(async move {
      let result = request.write(&target);
      (request, result)
    });
    self.editor_saves.insert(
      path.clone(),
      cx.spawn_in(window, async move |this, cx| {
        let (request, result) = work.await;
        let _ = this.update_in(cx, |this, window, cx| {
          this.editor_saves.remove(&path);
          match result {
            Ok(SaveOutcome::Saved(version)) => {
              this.editor_panel.update(cx, |editor, cx| editor.finish_save(&path, request, version, cx));
              crate::recent::record_file(&this.profile.id, &path, cx);
              this.refresh_in_background(cx);
            }
            Ok(SaveOutcome::Conflict(version)) if this.editor_panel.read(cx).active_file() == Some(path.as_path()) => {
              this.show_disk_conflict(path, version, None, window, cx);
            }
            Ok(SaveOutcome::Conflict(_)) => window.push_notification(
              crate::failure(format!("{} changed on disk. Open the file and save to resolve the conflict.", display_path(&path))),
              cx,
            ),
            Err(error) => window.push_notification(crate::failure(format!("Could not save {}: {error}", display_path(&path))), cx),
          }
        });
      }),
    );
  }

  fn show_disk_conflict(
    &self,
    path: PathBuf,
    version: Option<FileVersion>,
    diff: Option<Entity<DiffState>>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let workspace = cx.entity().downgrade();
    let missing = version.is_none();
    let title = if missing { "File was deleted from disk" } else { "File changed on disk" };
    let description: SharedString = format!("{}\nYour editor still has your text.", display_path(&path)).into();
    window.open_alert_dialog(cx, move |alert, _, _| {
      let cancel = Button::new("cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx));
      let compare = Button::new("compare").label("Compare Changes").on_click({
        let (workspace, path, version) = (workspace.clone(), path.clone(), version.clone());
        move |_, window, cx| {
          window.close_dialog(cx);
          let _ = workspace.update(cx, |this, cx| this.compare_disk_conflict(path.clone(), version.clone(), window, cx));
        }
      });
      let reload = Button::new("reload").label(if missing { "Discard Edits" } else { "Reload and Discard Edits" }).on_click({
        let (workspace, path, version) = (workspace.clone(), path.clone(), version.clone());
        move |_, window, cx| {
          window.close_dialog(cx);
          let _ = workspace.update(cx, |this, cx| this.reload_conflict(path.clone(), version.as_ref(), window, cx));
        }
      });
      let save_as = Button::new("save-as").label("Save As…").on_click({
        let (workspace, path) = (workspace.clone(), path.clone());
        move |_, window, cx| {
          window.close_dialog(cx);
          let _ = workspace.update(cx, |this, cx| this.save_conflict_copy(path.clone(), window, cx));
        }
      });
      let overwrite = Button::new("overwrite").danger().label(if missing { "Recreate File" } else { "Overwrite" }).on_click({
        let (workspace, path, version) = (workspace.clone(), path.clone(), version.clone());
        move |_, window, cx| {
          window.close_dialog(cx);
          let _ = workspace.update(cx, |this, cx| this.save_editor_file(path.clone(), Some(&version), window, cx));
        }
      });
      alert
        .title(title)
        .description(description.clone())
        .confirm()
        .w(rems(50.))
        .children(diff.as_ref().map(|state| Diff::new(state).h(rems(22.)).w_full()))
        .footer(DialogFooter::new().child(cancel).when(diff.is_none(), |footer| footer.child(compare)).child(reload).child(save_as).child(overwrite))
    });
  }

  fn compare_disk_conflict(&mut self, path: PathBuf, approved: Option<FileVersion>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(local) = self.editor_panel.read(cx).buffer_text(&path, cx) else { return };
    let input = path.clone();
    let work = cx.background_executor().spawn(async move {
      let (disk, current) = match read_editor_text(&input) {
        Ok((disk, version)) => (disk, Some(version)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && FileVersion::read(&input)?.is_none() => (String::new(), None),
        Err(error) => return Err(error),
      };
      let patch = similar::TextDiff::configure()
        .timeout(std::time::Duration::from_millis(200))
        .diff_lines(&disk, &local)
        .unified_diff()
        .header("On Disk", "In Editor")
        .to_string();
      let files = DiffFile::parse(&patch).map_err(std::io::Error::other)?;
      Ok::<_, std::io::Error>((current, files))
    });
    self.save_dialog_task = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        let (version, diff) = match result {
          Ok((version, files)) => (version, Some(cx.new(|cx| DiffState::new(files, cx)))),
          Err(error) => {
            window.push_notification(crate::failure(format!("Could not compare changes: {error}")), cx);
            (approved, None)
          }
        };
        this.show_disk_conflict(path, version, diff, window, cx);
      });
    }));
  }

  fn reload_conflict(&mut self, path: PathBuf, version: Option<&FileVersion>, window: &mut Window, cx: &mut Context<Self>) {
    match self.editor_panel.update(cx, |editor, cx| editor.reload_from_disk(&path, version, window, cx)) {
      Ok(true) => {}
      Ok(false) => match FileVersion::read(&path) {
        Ok(current) => self.show_disk_conflict(path, current, None, window, cx),
        Err(error) => window.push_notification(crate::failure(format!("Could not check the disk file: {error}")), cx),
      },
      Err(error) => window.push_notification(crate::failure(format!("Could not reload from disk: {error}")), cx),
    };
  }

  fn save_conflict_copy(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let Some(folder) = path.parent() else { return };
    let selected = cx.prompt_for_new_path(folder, path.file_name().and_then(|name| name.to_str()));
    self.save_dialog_task = Some(cx.spawn_in(window, async move |this, cx| {
      let selected = selected.await;
      let _ = this.update_in(cx, |this, window, cx| match selected {
        Ok(Ok(Some(destination))) if this.editor_saves.contains_key(&destination) => {
          window.push_notification(crate::failure("Wait for the current save to finish before choosing that destination."), cx);
        }
        Ok(Ok(Some(destination))) => match this.editor_panel.update(cx, |editor, cx| editor.save_copy(&path, &destination, cx)) {
          Ok(true) => {
            this.open_file_as(destination.clone(), false, window, cx);
            crate::recent::record_file(&this.profile.id, &destination, cx);
            this.refresh_in_background(cx);
          }
          Ok(false) => {}
          Err(error) => window.push_notification(crate::failure(format!("Could not save a copy: {error}")), cx),
        },
        Ok(Ok(None)) => {}
        Ok(Err(error)) => window.push_notification(crate::failure(format!("Could not open the Save As dialog: {error}")), cx),
        Err(_) => {}
      });
    }));
  }
}
