//! Git change markers and review controls for the active editor buffer.

use super::*;
use crate::{diff_view, theme::colors};
use gpui_kit::base::input::{GutterDecoration, GutterDecorationCollection, RopeExt as _};
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::diff::{DiffLinePosition, DiffSide};
use gpui_kit::component::{h_flex, label::Label, Disableable as _};
use ws_git::conflict_markers::{conflict_regions, ConflictRegion};
use ws_git::editor_diff::{apply_editor_hunk, editor_diff, EditorChange, EditorDiff, HunkAction};

pub(super) struct GitPreview {
  shown: diff_view::ShownDiff,
  patch: String,
  staged: bool,
}

pub(super) struct GitChanges {
  gutter: GutterDecorationCollection,
  diff: EditorDiff,
  conflict_gutter: GutterDecorationCollection,
  conflicts: Vec<ConflictRegion>,
}

impl GitChanges {
  pub(super) fn new(state: &mut EditorState, cx: &mut Context<EditorState>) -> Self {
    Self {
      gutter: state.create_gutter_decorations_collection(Vec::new(), cx),
      diff: EditorDiff::default(),
      conflict_gutter: state.create_gutter_decorations_collection(Vec::new(), cx),
      conflicts: Vec::new(),
    }
  }

  /// Marks the lines between the conflict markers, so the conflicts of a file left by a merge can be told apart.
  pub(super) fn update_conflicts(&mut self, conflicts: Vec<ConflictRegion>, state: &Entity<EditorState>, cx: &mut App) {
    if self.conflicts == conflicts {
      return;
    }
    let text = state.read(cx).text();
    let offset = |line| if line < text.lines_len() { text.line_start_offset(line) } else { text.len() };
    let color = colors(cx).conflicted;
    self.conflict_gutter.set(
      conflicts.iter().map(|region| GutterDecoration::new(offset(region.lines.start)..offset(region.lines.end)).with_color(color)).collect(),
      cx,
    );
    self.conflicts = conflicts;
  }

  pub(super) fn update(&mut self, diff: EditorDiff, state: &Entity<EditorState>, cx: &mut App) {
    if self.diff == diff {
      return;
    }
    let text = state.read(cx).text();
    let offset = |line| if line < text.lines_len() { text.line_start_offset(line) } else { text.len() };
    let marks: Vec<_> = diff.changes.iter().map(|change| (offset(change.lines.start)..offset(change.lines.end), change_color(change, cx))).collect();
    self.gutter.set(marks.iter().map(|(range, color)| GutterDecoration::new(range.clone()).with_color(*color)).collect(), cx);
    self.diff = diff;
  }
}

pub(crate) fn change_color(change: &EditorChange, cx: &App) -> Hsla {
  let palette = colors(cx);
  if change.lines.is_empty() {
    palette.deleted
  } else if change.removed {
    palette.gutter_modified
  } else {
    palette.new
  }
}

impl EditorPanel {
  pub(super) fn watch_git_changes(cx: &mut Context<Self>) -> Task<()> {
    cx.spawn(async move |this, cx| loop {
      cx.background_executor().timer(Duration::from_millis(700)).await;
      let Ok(snapshot) = this.read_with(cx, |this, cx| {
        let path = this.active_file()?;
        let file = this.files.get(path)?;
        let state = file.state.read(cx);
        (state.text().len() as u64 <= MAX_FILE_BYTES).then(|| (path.to_owned(), file.state.entity_id(), state.value()))
      }) else {
        break;
      };
      let Some((path, id, text)) = snapshot else { continue };
      let target = path.clone();
      let buffer = text.clone();
      let (diff, conflicts) =
        cx.background_executor().spawn(async move { (editor_diff(&target, &buffer).unwrap_or_default(), conflict_regions(&buffer)) }).await;
      if this
        .update(cx, |this, cx| {
          let Some(file) = this.files.get_mut(&path) else { return };
          if file.state.entity_id() != id || file.state.read(cx).value() != text {
            return;
          }
          if file.git.conflicts != conflicts {
            file.git.update_conflicts(conflicts, &file.state, cx);
            cx.notify();
          }
          if file.git.diff != diff {
            file.git.update(diff, &file.state, cx);
            file.minimap.update(cx, |map, cx| map.set_git_changes(&file.git.diff.changes, cx));
            cx.notify();
          }
        })
        .is_err()
      {
        break;
      }
    })
  }

  pub(super) fn git_controls(&self, path: &Path, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    let file = self.files.get(path)?;
    let git = &file.git.diff;
    let conflicts = file.git.conflicts.len();
    if conflicts == 0 && git.changes.is_empty() && git.staged.is_empty() && git.unstaged.is_empty() {
      return None;
    }
    let mut controls = h_flex().gap_1().flex_none();
    if conflicts > 0 {
      let color = colors(cx).conflicted;
      controls = controls.child(
        Label::new(format!("{conflicts} conflict{}", if conflicts == 1 { "" } else { "s" })).text_color(color).font_weight(FontWeight::SEMIBOLD),
      );
      for (id, label, icon, forward) in [
        ("previous-editor-conflict", "Previous conflict", IconName::ChevronUp, false),
        ("next-editor-conflict", "Next conflict", IconName::ChevronDown, true),
      ] {
        let path = path.to_owned();
        controls = controls.child(
          Button::new(id)
            .ghost()
            .xsmall()
            .icon(Icon::new(icon))
            .tooltip(label)
            .accessibility_label(label)
            .on_click(cx.listener(move |this, _, window, cx| this.jump_conflict(&path, forward, window, cx))),
        );
      }
    }
    for (id, label, icon, forward) in
      [("previous-editor-change", "Previous change", IconName::ChevronUp, false), ("next-editor-change", "Next change", IconName::ChevronDown, true)]
    {
      let path = path.to_owned();
      controls = controls.child(
        Button::new(id)
          .ghost()
          .xsmall()
          .icon(Icon::new(icon))
          .tooltip(label)
          .accessibility_label(label)
          .on_click(cx.listener(move |this, _, window, cx| this.jump_git_change(&path, forward, window, cx))),
      );
    }
    for (id, label, patch) in [("unstaged-editor-diff", "Unstaged", &git.unstaged), ("staged-editor-diff", "Staged", &git.staged)] {
      if patch.is_empty() {
        continue;
      }
      let path = path.to_owned();
      controls = controls.child(Button::new(id).ghost().xsmall().label(label).on_click(cx.listener(move |this, _, window, cx| {
        this.open_buffer_diff(&path, label, window, cx);
      })));
    }
    Some(controls)
  }

  pub(super) fn open_gutter_change(&mut self, path: &Path, point: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(file) = self.files.get(path) else { return };
    let state = file.state.read(cx);
    let bounds = state.input_bounds();
    if point.x < bounds.left() || point.x >= bounds.left() + window.rem_size() / 4. || !bounds.contains(&point) {
      return;
    }
    let Some(height) = state.line_height() else { return };
    if height <= Pixels::ZERO {
      return;
    }
    let row = (f32::from(point.y - bounds.top() - state.scroll_offset().y) / f32::from(height)).max(0.) as usize;
    let line = state.display_map().display_row_to_buffer_line(row);
    if !file.git.diff.changes.iter().any(|change| change.lines.contains(&line) || (change.lines.is_empty() && change.lines.start.abs_diff(line) <= 1))
    {
      return;
    }
    let kind = if file.git.diff.unstaged.is_empty() { "Staged" } else { "Unstaged" };
    self.reveal_position(path, line, 0, window, cx);
    self.open_buffer_diff(path, kind, window, cx);
  }

  /// Moves the cursor to the first conflict marker of a file, if it has one. Reads the buffer itself, since a file that was
  /// just opened has not been scanned yet.
  pub fn reveal_first_conflict(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
    let Some(file) = self.files.get(path) else { return };
    let text = file.state.read(cx).value().to_string();
    if let Some(region) = conflict_regions(&text).first() {
      self.reveal_position(path, region.lines.start, 0, window, cx);
    }
  }

  fn jump_conflict(&mut self, path: &Path, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
    let Some(file) = self.files.get(path) else { return };
    let line = file.state.read(cx).cursor_position().line as usize;
    let starts = || file.git.conflicts.iter().map(|region| region.lines.start);
    let target = if forward {
      starts().find(|start| *start > line).or_else(|| starts().next())
    } else {
      starts().rfind(|start| *start < line).or_else(|| starts().next_back())
    };
    if let Some(target) = target {
      self.reveal_position(path, target, 0, window, cx);
    }
  }

  fn jump_git_change(&mut self, path: &Path, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
    let Some(file) = self.files.get(path) else { return };
    let state = file.state.read(cx);
    let line = state.cursor_position().line as usize;
    let starts = || file.git.diff.changes.iter().map(|change| change.lines.start.min(state.text().lines_len().saturating_sub(1)));
    let target = if forward {
      starts().find(|start| *start > line).or_else(|| starts().next())
    } else {
      starts().rfind(|start| *start < line).or_else(|| starts().next_back())
    };
    if let Some(target) = target {
      self.reveal_position(path, target, 0, window, cx);
    }
  }

  fn open_buffer_diff(&mut self, path: &Path, label: &'static str, window: &mut Window, cx: &mut Context<Self>) {
    let Some(file) = self.files.get_mut(path) else { return };
    let staged = label == "Staged";
    let patch = if staged { &file.git.diff.staged } else { &file.git.diff.unstaged };
    if patch.is_empty() {
      return;
    }
    let files = match diff_view::parse_patch(patch) {
      Ok(files) => files,
      Err(error) => {
        window.push_notification(crate::failure(error), cx);
        return;
      }
    };
    let position = file.state.read(cx).cursor_position().line as usize + 1;
    let mut target = None;
    let mut distance = usize::MAX;
    diff_view::visit_patch_lines(patch, &files, |file, side, line, changed, _| {
      let next = line.abs_diff(position);
      if side == DiffSide::Modified && changed && next < distance {
        target = Some(DiffLinePosition::new(file.path().clone(), side, line));
        distance = next;
      }
    });
    let shown = diff_view::ShownDiff::new(files, patch, cx);
    if let Some(position) = target {
      shown.state().update(cx, |state, cx| state.scroll_to_line(position, cx));
    }
    file.git_preview = Some(GitPreview { shown, patch: patch.clone(), staged });
    cx.notify();
  }

  pub(super) fn render_git_preview(&self, path: &Path, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    let file = self.files.get(path)?;
    let preview = file.git_preview.as_ref()?;
    let pending = self.git_operation.is_some();
    let actions: &[(&str, &str, HunkAction)] = if preview.staged {
      &[("unstage-editor-hunk", "Unstage block", HunkAction::Unstage)]
    } else {
      &[("stage-editor-hunk", "Stage block", HunkAction::Stage), ("revert-editor-hunk", "Revert block…", HunkAction::Revert)]
    };
    let close = path.to_owned();
    let mut bar = h_flex().gap_2().px_2().py_1().child(if preview.staged { "Staged changes" } else { "Unstaged changes" }).child(div().flex_1());
    for &(id, label, action) in actions {
      let path = path.to_owned();
      bar = bar.child(
        Button::new(id)
          .ghost()
          .xsmall()
          .label(label)
          .disabled(pending || (!preview.staged && file.dirty))
          .tooltip(if !preview.staged && file.dirty { "Save this file first" } else { "Select a line in the changed block" })
          .on_click(cx.listener(move |this, _, window, cx| this.request_hunk_action(&path, action, window, cx))),
      );
    }
    bar =
      bar.child(Button::new("close-editor-diff").ghost().xsmall().icon(Icon::new(IconName::Close)).accessibility_label("Close changes").on_click(
        cx.listener(move |this, _, _, cx| {
          if let Some(file) = this.files.get_mut(&close) {
            file.git_preview = None;
          }
          cx.notify();
        }),
      ));
    Some(v_flex().flex_none().h_56().min_h_0().border_b_1().border_color(cx.theme().border).child(bar).child(div().flex_1().min_h_0().child(
      if preview.shown.is_ready() {
        preview.shown.with_controls(preview.shown.view(cx).size_full(), cx).into_any_element()
      } else {
        crate::components::skeleton_lines(16)
      },
    )))
  }

  fn request_hunk_action(&mut self, path: &Path, action: HunkAction, window: &mut Window, cx: &mut Context<Self>) {
    let Some(file) = self.files.get(path) else { return };
    if self.git_operation.is_some() || (file.dirty && !matches!(action, HunkAction::Unstage)) {
      return;
    }
    let Some(preview) = &file.git_preview else { return };
    let Some(selection) = preview.shown.state().read(cx).selected_lines().filter(|range| range.side() == range.end_side()) else {
      window.push_notification(crate::failure("Select a line in the changed block first."), cx);
      return;
    };
    let path = path.to_owned();
    let patch = preview.patch.clone();
    let lines = selection.start()..selection.end().saturating_add(1);
    let original = selection.side() == DiffSide::Original;
    if matches!(action, HunkAction::Revert) {
      let editor = cx.entity().downgrade();
      window.open_alert_dialog(cx, move |alert, _, _| {
        let (editor, path, patch, lines) = (editor.clone(), path.clone(), patch.clone(), lines.clone());
        alert
          .title("Revert this block?")
          .description("This removes the selected block of unstaged changes from the file.")
          .confirm()
          .ok_text("Revert block")
          .ok_variant(ButtonVariant::Danger)
          .on_ok(move |_, window, cx| {
            let _ = editor.update(cx, |this, cx| this.run_hunk_action(path.clone(), patch.clone(), lines.clone(), original, action, window, cx));
            true
          })
      });
    } else {
      self.run_hunk_action(path, patch, lines, original, action, window, cx);
    }
  }

  fn run_hunk_action(
    &mut self,
    path: PathBuf,
    patch: String,
    lines: Range<usize>,
    original: bool,
    action: HunkAction,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if self.git_operation.is_some() || self.files.get(&path).is_some_and(|file| file.dirty && !matches!(action, HunkAction::Unstage)) {
      return;
    }
    let target = path.clone();
    let work = cx
      .background_executor()
      .spawn(async move { apply_editor_hunk(&target, &patch, lines, original, action).map_err(|error| error.message().to_owned()) });
    self.git_operation = Some(cx.spawn_in(window, async move |this, cx| {
      let result = work.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.git_operation = None;
        match result {
          Ok(()) => {
            if let Some(file) = this.files.get_mut(&path) {
              file.git_preview = None;
            }
          }
          Err(error) => window.push_notification(crate::failure(error), cx),
        }
        cx.notify();
      });
    }));
    cx.notify();
  }
}
