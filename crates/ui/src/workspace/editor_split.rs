//! Ownership and focus for the optional second editable group.

use super::*;

impl Workspace {
  pub(super) fn editor_panels(&self) -> impl Iterator<Item = Entity<EditorPanel>> + use<> {
    std::iter::once(self.editor_panel.clone()).chain(self.split_editor.clone())
  }

  pub(super) fn focused_editor(&self, window: &Window, cx: &App) -> Entity<EditorPanel> {
    self
      .split_editor
      .as_ref()
      .filter(|editor| {
        self.split_open
          && (editor.read(cx).focus_handle(cx).contains_focused(window, cx)
            || (self.split_focused && !self.editor_panel.read(cx).focus_handle(cx).contains_focused(window, cx)))
      })
      .cloned()
      .unwrap_or_else(|| self.editor_panel.clone())
  }

  pub(super) fn editor_for_path(&self, path: &Path, cx: &App) -> Entity<EditorPanel> {
    self.split_editor.as_ref().filter(|editor| editor.read(cx).contains_buffer(path)).cloned().unwrap_or_else(|| self.editor_panel.clone())
  }

  pub(super) fn target_editor(&mut self, path: &Path, window: &Window, cx: &mut Context<Self>) -> Entity<EditorPanel> {
    let owner = self.editor_for_path(path, cx);
    if owner != self.editor_panel {
      self.split_open = true;
      return owner;
    }
    let focused = self.focused_editor(window, cx);
    if focused != self.editor_panel && !self.editor_panel.read(cx).protects_buffer(path) && !self.editor_panel.read(cx).recovery_is_loading() {
      if self.editor_panel.read(cx).contains_buffer(path) {
        self.editor_panel.update(cx, |editor, cx| editor.forget(path, cx));
      }
      focused
    } else {
      owner
    }
  }

  pub(super) fn open_definition(&mut self, target: &OpenLocation, window: &mut Window, cx: &mut Context<Self>) {
    let path = &target.path;
    let target_editor = self.target_editor(path, window, cx);
    target_editor.update(cx, |editor, _| editor.allow_large(path));
    self.open_file(path.clone(), window, cx);
    target_editor.update(cx, |editor, cx| editor.reveal_range(path, target.start, target.end, window, cx));
    target_editor.update(cx, |editor, cx| editor.focus_document(window, cx));
  }

  pub(super) fn toggle_editor_split(&mut self, _: &crate::ToggleEditorSplit, window: &mut Window, cx: &mut Context<Self>) {
    self.split_open = !self.split_open;
    self.split_focused = self.split_open;
    if self.split_open {
      self.reveal_editor_groups(window, cx);
      self.ensure_split_editor(window, cx).update(cx, |editor, cx| editor.focus_document(window, cx));
    } else {
      self.editor_panel.update(cx, |editor, cx| editor.focus_document(window, cx));
    }
    cx.notify();
  }

  fn reveal_editor_groups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.sidebar_tab = SidebarTab::Explorer;
    self.leave_project_settings();
    self.app_settings = None;
    self.agent_open = false;
    if !self.terminal_docked {
      self.hide_terminal(window, cx);
    }
  }

  fn ensure_split_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<EditorPanel> {
    if let Some(editor) = &self.split_editor {
      return editor.clone();
    }
    let editor = cx.new(|cx| EditorPanel::new(window, cx));
    let root = self.projects.active.clone();
    let trusted = self.editor_panel.read(cx).trusted_root();
    editor.update(cx, |editor, cx| {
      editor.share_recovery(&self.editor_panel, window, cx);
      editor.set_root(root.clone(), cx);
      for project in &self.projects.projects {
        editor.set_editor_preferences(project.path.clone(), project.editor, cx);
      }
      if let (Some(root), Some(trusted)) = (root, trusted) {
        editor.enable_language_servers(root, trusted, window, cx);
      }
    });
    self.split_subscriptions = vec![
      cx.on_focus_in(&editor.read(cx).focus_handle(cx), window, |this, _, cx| {
        this.split_focused = true;
        cx.notify();
      }),
      cx.observe(&editor, |_, _, cx| cx.notify()),
      cx.subscribe_in(&editor, window, |this, editor, shown: &ActiveTab, window, cx| {
        if let ActiveTab::File(path) = shown {
          editor.update(cx, |editor, cx| editor.sync_shown_file(path, window, cx));
          if let Some(root) = this.projects.active.clone().filter(|root| path.starts_with(root)) {
            this.show_explorer_entry(&root, path.clone(), ScrollStrategy::Nearest, cx);
          }
        }
      }),
      cx.subscribe_in(&editor, window, |this, _, target: &OpenLocation, window, cx| {
        this.open_definition(target, window, cx);
      }),
    ];
    self.split_editor = Some(editor.clone());
    editor
  }

  pub(super) fn edit_in_split(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    if self.editor_panel.read(cx).recovery_is_loading() {
      window.push_notification(Notification::info("Wait for editor recovery to finish, then try again."), cx);
      return;
    }
    if self.editor_panel.read(cx).protects_buffer(&path) {
      self.reveal_editor_groups(window, cx);
      self.open_file_as(path, false, window, cx);
      self.editor_panel.update(cx, |editor, cx| editor.focus_document(window, cx));
      window.push_notification(Notification::info("This file is already open in the first editor."), cx);
      return;
    }
    let editor = self.ensure_split_editor(window, cx);
    if self.editor_panel.read(cx).contains_buffer(&path) {
      self.editor_panel.update(cx, |editor, cx| editor.forget(&path, cx));
    }
    self.split_open = true;
    self.split_focused = true;
    self.reveal_editor_groups(window, cx);
    match editor.update(cx, |editor, cx| editor.open_file(path.clone(), window, cx)) {
      Ok(()) => {
        self.selected_entry = Some(path);
        self.selected_change = None;
        editor.update(cx, |editor, cx| editor.focus_document(window, cx));
      }
      Err(message) => window.push_notification(crate::failure(message), cx),
    }
    cx.notify();
  }
}
