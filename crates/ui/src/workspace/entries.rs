//! Explorer entries and opening files.

use super::*;

impl Workspace {
  /// Asks for the name of a new file or folder inside the selected folder, next to the selected file, or in the project folder.
  pub(super) fn new_entry(&mut self, is_dir: bool, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let parent = self
      .selected_entry
      .as_deref()
      .map(|entry| if entry.is_dir() { entry } else { entry.parent().unwrap_or(entry) })
      .filter(|folder| folder.starts_with(&root))
      .map_or_else(|| root.clone(), Path::to_path_buf);
    let (kind, placeholder) = if is_dir { ("folder", "Folder name") } else { ("file", "File name") };
    let place = parent.file_name().map_or_else(|| "project".into(), |name| name.to_string_lossy().into_owned());
    self.ask_name(format!("New {kind} in {place}"), placeholder, "Press Enter to create it.", "", window, cx, move |this, name, window, cx| {
      let path = explorer::create(&parent, name, is_dir)?;
      this.entry_created(path, is_dir, window, cx);
      Ok(())
    });
  }

  /// Opens a dialog with one text field. Enter hands the text to `submit`; when that succeeds the dialog closes,
  /// and when it fails the message shows as a notification and the dialog stays.
  pub(super) fn ask_name(
    &mut self,
    title: impl Into<SharedString>,
    placeholder: &'static str,
    hint: &'static str,
    initial: &str,
    window: &mut Window,
    cx: &mut Context<Self>,
    submit: impl Fn(&mut Self, &str, &mut Window, &mut Context<Self>) -> Result<(), String> + 'static,
  ) {
    let initial = initial.to_owned();
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(initial));
    self._dialog_subscription = Some(cx.subscribe_in(&input, window, move |this, input, event: &InputEvent, window, cx| {
      if let InputEvent::PressEnter { .. } = event {
        let name = input.read(cx).value().to_string();
        match submit(this, &name, window, cx) {
          Ok(()) => window.close_dialog(cx),
          Err(message) => window.push_notification(crate::failure(message), cx),
        }
      }
    }));
    let title = title.into();
    let field = input.clone();
    let muted = cx.theme().muted_foreground;
    window.open_dialog(cx, move |dialog, _, cx| {
      dialog
        .title(title.clone())
        .width(px(420.))
        .p_5()
        .child(v_flex().gap_2().child(crate::components::soft_input(&field, cx)).child(Label::new(hint).text_size(FONT_SIZE_SM).text_color(muted)))
    });
    let focus = input.read(cx).focus_handle(cx);
    window.defer(cx, move |window, cx| focus.focus(window, cx));
  }

  /// Shows a new file or folder in the tree, and opens a new file.
  pub(super) fn entry_created(&mut self, path: PathBuf, is_dir: bool, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(root) = self.projects.active.clone() {
      for folder in path.ancestors().skip(1).take_while(|folder| *folder != root && folder.starts_with(&root)) {
        self.expanded.insert(folder.to_path_buf());
      }
    }
    self.refresh_project();
    if is_dir {
      cx.notify();
    } else {
      self.open_file(path, window, cx);
    }
  }

  /// Folds every folder of the tree.
  pub(super) fn collapse_folders(&mut self, cx: &mut Context<Self>) {
    self.expanded.clear();
    self.refresh_project();
    cx.notify();
  }

  pub(super) fn toggle_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    if !self.expanded.remove(&path) {
      self.expanded.insert(path);
    }
    self.refresh_project();
    cx.notify();
  }

  /// Opens a file from the Explorer in the editor.
  pub(super) fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    self.open_file_as(path, false, window, cx);
  }

  /// Shift selects the rows between the anchor and `path`; Command adds or removes `path`.
  pub(super) fn select_entries(&mut self, path: PathBuf, range: bool, cx: &mut Context<Self>) {
    let position = |target: &Path| self.rows.iter().position(|row| row.path == target);
    let anchor = self.selection_anchor.clone().or_else(|| self.selected_entry.clone());
    if range {
      if let (Some(from), Some(to)) = (anchor.as_deref().and_then(position), position(&path)) {
        let (from, to) = (from.min(to), from.max(to));
        self.explorer_selection = self.rows[from..=to].iter().map(|row| row.path.clone()).collect();
        self.selection_anchor = anchor;
        self.selected_entry = Some(path);
        cx.notify();
        return;
      }
    }
    if self.explorer_selection.is_empty() {
      self.explorer_selection.extend(self.selected_entry.clone());
    }
    if !self.explorer_selection.remove(&path) {
      self.explorer_selection.insert(path.clone());
    }
    self.selection_anchor = Some(path.clone());
    self.selected_entry = Some(path);
    cx.notify();
  }

  /// Opens a file; `preview` shows it in the preview tab, which the next preview replaces.
  pub(super) fn open_file_as(&mut self, path: PathBuf, preview: bool, window: &mut Window, cx: &mut Context<Self>) {
    let result =
      self.editor_panel.update(
        cx,
        |editor, cx| {
          if preview {
            editor.open_preview(path.clone(), window, cx)
          } else {
            editor.open_file(path.clone(), window, cx)
          }
        },
      );
    match result {
      Ok(()) => {
        self.explorer_selection.clear();
        self.selected_entry = Some(path);
        self.selected_change = None;
      }
      Err(message) => window.push_notification(crate::failure(message), cx),
    }
    cx.notify();
  }

  pub(super) fn open_changed_file(&mut self, action: &OpenChangedFile, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else { return };
    let Some(path) = ws_git::workdir_file(&root, Path::new(action.0.as_str())) else {
      window.push_notification(crate::failure("Could not locate the file in the working tree."), cx);
      return;
    };
    // Git may resolve the project directory through a symlink. Keep Explorer's path spelling.
    let path = if path.starts_with(&root) {
      path
    } else {
      root.canonicalize().ok().and_then(|canonical| path.strip_prefix(canonical).ok()).map(|relative| root.join(relative)).unwrap_or(path)
    };
    self.reveal_file(root, path, window, cx);
  }

  /// Opens a file in the editor and shows it in the Explorer: its folders open and the tree scrolls to it.
  pub(super) fn reveal_file(&mut self, root: PathBuf, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    self.sidebar_tab = SidebarTab::Explorer;
    self.close_project_search(cx);
    self.open_file(path.clone(), window, cx);
    if self.selected_entry.as_ref() == Some(&path) {
      self.show_explorer_entry(&root, path, ScrollStrategy::Center, cx);
    }
  }

  pub(super) fn show_explorer_entry(&mut self, root: &Path, path: PathBuf, strategy: ScrollStrategy, cx: &mut Context<Self>) {
    if !path.starts_with(root) {
      return;
    }
    let expanded = self.expanded.len();
    self.expanded.extend(path.ancestors().skip(1).take_while(|parent| *parent != root).map(Path::to_path_buf));
    self.explorer_selection.clear();
    self.selected_entry = Some(path.clone());
    if self.expanded.len() != expanded {
      self.refresh_with_explorer_reveal(Some((path, strategy)), cx);
    } else {
      self.scroll_to_explorer_entry(&path, strategy);
    }
  }

  pub(super) fn scroll_to_explorer_entry(&self, path: &Path, strategy: ScrollStrategy) {
    if let Some(index) = self.rows.iter().position(|row| row.path == path) {
      if strategy == ScrollStrategy::Center {
        self.explorer_scroll.scroll_to_item_strict(index, strategy);
      } else {
        self.explorer_scroll.scroll_to_item(index, strategy);
      }
    }
  }
}
