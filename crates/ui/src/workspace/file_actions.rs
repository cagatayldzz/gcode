//! Explorer file commands and their context menus.

use super::*;
use gpui_kit::component::menu::PopupMenu;
use std::{fs, process::Command};

#[derive(Clone, Copy, PartialEq)]
enum CommandKind {
  Preview,
  Side,
  OpenWith,
  Reveal,
  Timeline,
  Cut,
  Copy,
  Paste,
  CopyPath,
  CopyRelativePath,
  Rename,
  Delete,
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = vekro, no_json)]
pub(super) struct FileCommand {
  kind: CommandKind,
  root: PathBuf,
  path: PathBuf,
  directory: bool,
}

#[derive(Clone)]
struct FileClipboard {
  path: PathBuf,
  marker: String,
  cut: bool,
}

#[derive(Default)]
pub(super) struct FileActions {
  clipboard: Option<FileClipboard>,
  side: Option<Entity<EditorPanel>>,
  side_observer: Option<Subscription>,
  mutation: Option<Task<()>>,
  timeline: Option<Task<()>>,
  system_open: Option<Task<()>>,
}

impl FileActions {
  pub(super) fn clear_side(&mut self) {
    self.side = None;
    self.side_observer = None;
    self.timeline = None;
  }

  pub(super) fn visible_side(&self, cx: &App) -> Option<Entity<EditorPanel>> {
    self.side.clone().filter(|editor| editor.read(cx).has_tabs())
  }

  pub(super) fn focused_side(&self, window: &Window, cx: &App) -> Option<Entity<EditorPanel>> {
    self.visible_side(cx).filter(|editor| editor.read(cx).focus_handle(cx).contains_focused(window, cx))
  }

  fn clipboard_file(&self, cx: &App) -> Option<&FileClipboard> {
    let file = self.clipboard.as_ref()?;
    (cx.read_from_clipboard()?.metadata() == Some(&file.marker)).then_some(file)
  }
}

impl Workspace {
  pub(super) fn explorer_menu(
    &self,
    target: Option<&explorer::Row>,
    cx: &Context<Self>,
  ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + use<> {
    let root = self.projects.active.clone();
    let target = target.map(|row| (row.path.clone(), row.is_dir));
    let focus = self.focus_handle.clone();
    let workspace = cx.entity().downgrade();
    move |menu, _, cx| {
      let Some(root) = &root else { return menu };
      let (path, directory) = target.as_ref().map_or((root, true), |(path, directory)| (path, *directory));
      let can_paste = workspace.upgrade().is_some_and(|workspace| workspace.read(cx).file_actions.clipboard_file(cx).is_some());
      file_menu(menu.action_context(focus.clone()), root, path, directory, can_paste)
    }
  }

  pub(super) fn file_mutation_pending(&self) -> bool {
    self.file_actions.mutation.is_some()
  }

  pub(super) fn run_file_location(&mut self, action: &FileLocation, window: &mut Window, cx: &mut Context<Self>) {
    let FileLocation(command, path) = action;
    let Some(root) = self.projects.active.clone().filter(|root| path.starts_with(root)) else { return };
    match command {
      FileLocationCommand::CopyPath | FileLocationCommand::CopyRelativePath => {
        let copied = if *command == FileLocationCommand::CopyPath { path.as_path() } else { path.strip_prefix(&root).unwrap_or(path) };
        cx.write_to_clipboard(ClipboardItem::new_string(copied.to_string_lossy().into_owned()));
      }
      FileLocationCommand::RevealInFinder => self.open_in_system(path.clone(), true, window, cx),
      FileLocationCommand::RevealInExplorer => {
        self.sidebar_tab = SidebarTab::Explorer;
        self.show_explorer_entry(&root, path.clone(), ScrollStrategy::Center, cx);
        cx.notify();
      }
    }
  }

  pub(super) fn run_file_command(&mut self, command: &FileCommand, window: &mut Window, cx: &mut Context<Self>) {
    use CommandKind::*;
    let FileCommand { kind, root, path, directory } = command;
    if self.projects.active.as_ref() != Some(root) || !path.starts_with(root) {
      return;
    }
    match kind {
      Preview => self.open_file_as(path.clone(), true, window, cx),
      Side => self.open_side_preview(path.clone(), window, cx),
      OpenWith => self.open_in_system(path.clone(), false, window, cx),
      Reveal => self.run_file_location(&FileLocation(FileLocationCommand::RevealInFinder, path.clone()), window, cx),
      Timeline => self.open_file_timeline(path.clone(), window, cx),
      Cut | Copy => {
        let marker = format!("vekro-file-{}-{:?}", std::process::id(), Instant::now());
        cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(path.to_string_lossy().into_owned(), marker.clone()));
        self.file_actions.clipboard = Some(FileClipboard { path: path.clone(), marker, cut: *kind == Cut });
      }
      CopyPath | CopyRelativePath => {
        let command = if *kind == CopyPath { FileLocationCommand::CopyPath } else { FileLocationCommand::CopyRelativePath };
        self.run_file_location(&FileLocation(command, path.clone()), window, cx);
      }
      Paste => {
        let Some(source) = self.file_actions.clipboard_file(cx).cloned() else {
          window.push_notification(Notification::info("Cut or copy a file in Explorer before pasting."), cx);
          return;
        };
        let folder = if *directory { path.as_path() } else { path.parent().unwrap_or(root) };
        let Some(name) = source.path.file_name() else { return };
        let target = folder.join(name);
        let edit = if source.cut {
          FileEdit::Move { target, clipboard: Some(source.marker) }
        } else {
          FileEdit::Copy { target, duplicate: source.path.parent() == Some(folder) }
        };
        self.edit_file(source.path, edit, window, cx);
      }
      Rename => {
        let Some(name) = path.file_name() else { return };
        let (root, path) = (root.clone(), path.clone());
        let initial = name.to_string_lossy().into_owned();
        self.ask_name("Rename file", "File name", "Press Enter to rename it.", &initial, window, cx, move |this, name, window, cx| {
          if this.projects.active.as_ref() != Some(&root) {
            return Err("The active project changed. Open Rename from the current project.".into());
          }
          let name = name.trim();
          if name.is_empty() || name.contains('/') || name == "." || name == ".." {
            return Err("Enter a single file name.".into());
          }
          let target = path.with_file_name(name);
          if target != path {
            this.edit_file(path.clone(), FileEdit::Move { target, clipboard: None }, window, cx);
          }
          Ok(())
        });
      }
      Delete => {
        let workspace = cx.entity().downgrade();
        let (root, path) = (root.clone(), path.clone());
        let title = SharedString::from(format!("Delete {}?", display_path(&path)));
        window.open_alert_dialog(cx, move |alert, _, _| {
          let (workspace, root, path) = (workspace.clone(), root.clone(), path.clone());
          alert
            .title(title.clone())
            .description("The file will be moved to the Trash.")
            .confirm()
            .ok_text("Delete")
            .ok_variant(ButtonVariant::Danger)
            .on_ok(move |_, window, cx| {
              let _ = workspace.update(cx, |this, cx| {
                if this.projects.active.as_ref() == Some(&root) {
                  this.edit_file(path.clone(), FileEdit::Delete, window, cx);
                }
              });
              true
            })
        });
      }
    }
  }

  fn edit_file(&mut self, path: PathBuf, edit: FileEdit, window: &mut Window, cx: &mut Context<Self>) {
    if self.file_mutation_pending() || self.replacement_task.is_some() {
      window.push_notification(Notification::info("Wait for the current file operation to finish."), cx);
      return;
    }
    let moves_source = !matches!(edit, FileEdit::Copy { .. });
    if moves_source {
      if self.editor_panel.read(cx).is_dirty(&path) {
        window.push_notification(Notification::error("Save the file before moving or deleting it."), cx);
        return;
      }
      self.editor_panel.update(cx, |editor, cx| editor.lock_replacement_files([path.clone()], cx));
    }
    let root = self.projects.active.clone();
    let work = cx.background_executor().spawn({
      let path = path.clone();
      async move { edit.apply(&path) }
    });
    self.file_actions.mutation = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.file_actions.mutation = None;
        if moves_source {
          this.editor_panel.update(cx, |editor, cx| editor.lock_replacement_files([], cx));
        }
        match result {
          Ok((target, marker)) => {
            if let Some(marker) = marker {
              if this.file_actions.clipboard.as_ref().is_some_and(|file| file.marker == marker) {
                this.file_actions.clipboard = None;
              }
            }
            if moves_source {
              this.editor_panel.update(cx, |editor, cx| editor.forget(&path, cx));
              if let Some(side) = &this.file_actions.side {
                side.update(cx, |editor, cx| editor.forget(&path, cx));
              }
              for view in this.views.values_mut() {
                if view.selected_entry.as_ref() == Some(&path) {
                  view.selected_entry = None;
                }
              }
            }
            if this.projects.active == root {
              if this.selected_entry.as_ref() == Some(&path) || target.is_some() {
                this.selected_entry = target;
              }
              this.refresh_project();
            } else if moves_source && this.selected_entry.as_ref() == Some(&path) {
              this.selected_entry = None;
              this.refresh_project();
            }
            cx.notify();
          }
          Err(error) => window.push_notification(Notification::error(format!("File operation failed: {error}")), cx),
        }
      });
    }));
  }

  fn open_side_preview(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let panel = self.file_actions.side.get_or_insert_with(|| {
      let editor = cx.new(|cx| EditorPanel::side_preview(window, cx));
      self.file_actions.side_observer = Some(cx.observe(&editor, |_, _, cx| cx.notify()));
      editor
    });
    match panel.update(cx, |editor, cx| editor.open_file(path.clone(), window, cx)) {
      Ok(()) => {
        self.selected_entry = Some(path);
        let focus = panel.read(cx).focus_handle(cx);
        window.defer(cx, move |window, cx| focus.focus(window, cx));
        cx.notify();
      }
      Err(error) => window.push_notification(Notification::error(error), cx),
    }
  }

  fn open_in_system(&mut self, path: PathBuf, reveal: bool, window: &mut Window, cx: &mut Context<Self>) {
    if self.file_actions.system_open.is_some() {
      return;
    }
    let picker = (!reveal).then(|| {
      cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Choose Application".into()) })
    });
    self.file_actions.system_open = Some(cx.spawn_in(window, async move |this, cx| {
      let result = async {
        let mut command = Command::new("/usr/bin/open");
        if let Some(picker) = picker {
          let paths = picker.await.map_err(|error| error.to_string())?.map_err(|error| error.to_string())?;
          let Some(application) = paths.and_then(|paths| paths.into_iter().next()) else { return Ok(()) };
          if application.extension().and_then(|extension| extension.to_str()).is_none_or(|extension| !extension.eq_ignore_ascii_case("app")) {
            return Err("Choose a macOS application (.app).".to_owned());
          }
          command.arg("-a").arg(application);
        } else {
          command.arg("-R");
        }
        command.arg(path);
        cx.background_executor()
          .spawn(async move {
            let output = command.output().map_err(|error| error.to_string())?;
            if !output.status.success() {
              return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
            }
            Ok(())
          })
          .await
      }
      .await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.file_actions.system_open = None;
        if let Err(error) = result {
          window.push_notification(Notification::error(format!("Could not open the file: {error}")), cx);
        }
      });
    }));
  }

  fn open_file_timeline(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    let title = SharedString::from(format!("Timeline · {}", display_path(&path)));
    let work = cx.background_executor().spawn(async move { vekro_git::file_history(&path, 100) });
    self.file_actions.timeline = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.file_actions.timeline = None;
        match result {
          Ok(commits) => window.open_dialog(cx, move |dialog, window, _| {
            dialog.title(title.clone()).width(window.rem_size() * 36.).p_5().child(
              v_flex()
                .id("file-timeline")
                .max_h(rems(24.))
                .overflow_y_scrollbar()
                .gap_2()
                .when(commits.is_empty(), |list| list.child(Label::new("No commits for this path.")))
                .children(commits.iter().map(|commit| {
                  v_flex()
                    .id(SharedString::from(commit.id.clone()))
                    .gap_0p5()
                    .child(Label::new(commit.summary.clone()).font_weight(FontWeight::MEDIUM))
                    .child(Label::new(format!("{} · {} · {}", commit.short, commit.author, ago_long(commit.time))).text_size(FONT_SIZE_SM))
                })),
            )
          }),
          Err(error) => window.push_notification(Notification::error(format!("Could not load the file timeline: {error}")), cx),
        }
      });
    }));
  }
}

fn file_menu(menu: PopupMenu, root: &Path, path: &Path, directory: bool, can_paste: bool) -> PopupMenu {
  use CommandKind::*;
  let item = |label, kind| PopupMenuItem::new(label).action(Box::new(FileCommand { kind, root: root.into(), path: path.into(), directory }));
  if directory {
    return menu.item(item("Paste", Paste).disabled(!can_paste));
  }
  menu
    .item(item("Open Preview", Preview))
    .item(item("Open to the Side", Side))
    .item(item("Open With…", OpenWith))
    .item(item("Reveal in Finder", Reveal))
    .separator()
    .item(item("Open Timeline", Timeline))
    .separator()
    .item(item("Cut", Cut))
    .item(item("Copy", Copy))
    .item(item("Paste", Paste).disabled(!can_paste))
    .separator()
    .item(item("Copy Path", CopyPath))
    .item(item("Copy Relative Path", CopyRelativePath))
    .separator()
    .item(item("Rename…", Rename))
    .item(item("Delete…", Delete))
}

enum FileEdit {
  Move { target: PathBuf, clipboard: Option<String> },
  Copy { target: PathBuf, duplicate: bool },
  Delete,
}

impl FileEdit {
  fn apply(self, source: &Path) -> std::io::Result<(Option<PathBuf>, Option<String>)> {
    let metadata = fs::symlink_metadata(source)?;
    if !(metadata.is_file() || metadata.is_symlink()) {
      return Err(std::io::Error::other("Select a file."));
    }
    match self {
      Self::Move { target, clipboard } => {
        explorer::move_file(source, &target)?;
        Ok((Some(target), clipboard))
      }
      Self::Copy { target, duplicate } => {
        let target = explorer::copy_file(source, &target, &metadata, duplicate)?;
        Ok((Some(target), None))
      }
      Self::Delete => {
        trash_file(source)?;
        Ok((None, None))
      }
    }
  }
}

#[cfg(target_os = "macos")]
fn trash_file(source: &Path) -> std::io::Result<()> {
  use objc2_foundation::{NSFileManager, NSURL};
  let url = NSURL::from_file_path(source).ok_or_else(|| std::io::Error::other("Could not represent the file as a URL."))?;
  NSFileManager::defaultManager()
    .trashItemAtURL_resultingItemURL_error(&url, None)
    .map_err(|error| std::io::Error::other(error.localizedDescription().to_string()))
}

#[cfg(not(target_os = "macos"))]
fn trash_file(_: &Path) -> std::io::Result<()> {
  Err(std::io::Error::other("Trash is unavailable on this platform."))
}
