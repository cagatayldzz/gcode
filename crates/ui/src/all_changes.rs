//! The working tree's changes in one virtualized Kit diff. Each file's header carries the review controls.

use crate::{
  diff_view::{visit_patch_lines, ShownDiff},
  settings::{code_font_size, SettingsModel},
  theme::{colors, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    clipboard::Clipboard,
    diff::{DiffFile, DiffGutterMarker, DiffLinePosition, DiffState},
    h_flex,
    label::Label,
    menu::{DropdownMenu as _, PopupMenuItem},
    progress::ProgressCircle,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, StyledExt as _,
  },
  *,
};
use std::{
  cell::RefCell,
  collections::{hash_map::DefaultHasher, HashMap, HashSet},
  hash::{Hash, Hasher},
  path::PathBuf,
  rc::Rc,
  sync::Arc,
};

// The Kit adds a one-pixel separator above every header after the first one.
const FILE_HEADER_SEPARATOR: Pixels = px(1.);

#[derive(Default)]
struct FileHeaderLayout {
  above: Option<SharedString>,
  below: Option<(SharedString, Pixels)>,
  first_line: Option<(SharedString, Pixels)>,
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = app, no_json)]
pub(crate) struct OpenChangedFile(pub SharedString);

pub(crate) struct AllChanges {
  /// The settings of the profile, for the size of code.
  settings: Entity<SettingsModel>,
  diff: Option<ShownDiff>,
  patch: Option<Arc<str>>,
  /// The files marked as viewed, with the fingerprint of their change at that time: a file that changes afterwards needs a new review.
  viewed: HashMap<SharedString, u64>,
  /// The fingerprint of each file's change in the diff on screen.
  fingerprints: HashMap<SharedString, u64>,
  error: Option<SharedString>,
  task: Option<Task<()>>,
  current_file: Rc<RefCell<Option<SharedString>>>,
  line_probes: Vec<DiffGutterMarker>,
}

impl AllChanges {
  pub fn new(settings: Entity<SettingsModel>) -> Self {
    Self {
      settings,
      diff: None,
      patch: None,
      viewed: HashMap::new(),
      fingerprints: HashMap::new(),
      error: None,
      task: None,
      current_file: Rc::default(),
      line_probes: Vec::new(),
    }
  }

  pub fn reset(&mut self, cx: &mut Context<Self>) {
    *self = Self::new(self.settings.clone());
    cx.notify();
  }

  pub fn reload(&mut self, root: PathBuf, cx: &mut Context<Self>) {
    if self.task.is_some() {
      return;
    }
    let previous = self.patch.clone();
    let work = cx.background_executor().spawn(async move {
      let patch = ws_git::all_changes_diff(&root)?;
      if previous.as_deref() == Some(patch.as_str()) {
        return Ok(None);
      }
      let files = crate::diff_view::parse_patch(&patch)?;
      let line_probes = line_probes(&patch, &files);
      let fingerprints = file_fingerprints(&patch, &files);
      Ok::<_, String>(Some((patch, files, line_probes, fingerprints)))
    });
    self.task = Some(cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        this.task = None;
        match result {
          Ok(None) => return,
          Ok(Some((patch, files, line_probes, fingerprints))) => {
            this.line_probes = line_probes;
            // A file whose change differs from the one that was reviewed loses its mark.
            this.viewed.retain(|path, fingerprint| fingerprints.get(path) == Some(fingerprint));
            this.fingerprints = fingerprints;
            if let Some(diff) = &mut this.diff {
              diff.set_files(files, &patch, cx);
            } else {
              this.diff = Some(ShownDiff::new(files, &patch, cx));
            }
            this.patch = Some(patch.into());
            this.error = None;
          }
          Err(error) => {
            this.patch = None;
            this.error = Some(format!("Could not read all changes: {error}").into());
          }
        }
        cx.notify();
      });
    }));
  }
}

impl AllChanges {
  /// Scrolls to a file's header and opens it, whether it was folded or not.
  pub fn scroll_to(&mut self, path: &str, cx: &mut Context<Self>) {
    if let Some(diff) = &self.diff {
      reveal_file(diff.state(), path, cx);
    }
  }

  /// Jumps to the first file not marked as viewed.
  fn next_unviewed(&mut self, cx: &mut Context<Self>) {
    let Some(diff) = &self.diff else { return };
    let next = diff.state().read(cx).files().iter().map(|file| file.path().clone()).find(|path| !self.viewed.contains_key(path));
    if let Some(path) = next {
      self.scroll_to(&path, cx);
    }
  }

  /// Folds every file to its header, or opens them all.
  fn set_all_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
    if let Some(diff) = &self.diff {
      diff.state().update(cx, |state, cx| state.set_all_files_collapsed(collapsed, cx));
    }
  }

  /// The bar above the diff: what changed in total, the jump menu, the review shortcuts and the progress.
  fn toolbar(&self, state: &Entity<DiffState>, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let muted = cx.theme().muted_foreground;
    let files: Vec<(SharedString, usize, usize)> =
      state.read(cx).files().iter().map(|file| (file.path().clone(), file.additions(), file.deletions())).collect();
    let (count, viewed) = (files.len(), self.viewed.len());
    let (added, removed) = files.iter().fold((0, 0), |(added, removed), (_, a, d)| (added + a, removed + d));
    let all_viewed = viewed >= count;
    let reviewed: HashSet<SharedString> = self.viewed.keys().cloned().collect();
    let menu_state = state.clone();
    let jump = Button::new("all-changes-files")
      .ghost()
      .small()
      .icon(Icon::empty().path("icons/files.svg"))
      .label("Jump to file")
      .child(Icon::new(IconName::ChevronDown).size_3())
      .dropdown_menu(move |menu, _, _| {
        files.iter().fold(menu.scrollable(true).max_h(px(360.)), |menu, (path, added, removed)| {
          let (target, state) = (path.clone(), menu_state.clone());
          menu.item(PopupMenuItem::new(format!("{path}   +{added} −{removed}")).checked(reviewed.contains(path)).on_click(move |_, _, cx| {
            reveal_file(&state, &target, cx);
          }))
        })
      });
    h_flex()
      .flex_none()
      .min_w_0()
      .overflow_hidden()
      .gap_3()
      .px_4()
      .py_1p5()
      .items_center()
      .border_b_1()
      .border_color(palette.border)
      .bg(palette.surface_alt)
      .child(jump)
      .child(
        h_flex()
          .gap_2()
          .items_center()
          .text_size(FONT_SIZE_SM)
          .child(Label::new(if count == 1 { "1 file".to_string() } else { format!("{count} files") }).text_color(muted))
          .child(div().text_color(palette.new).child(format!("+{added}")))
          .child(div().text_color(palette.deleted).child(format!("−{removed}"))),
      )
      .child(div().flex_1())
      .child(
        Button::new("all-changes-next")
          .ghost()
          .small()
          .icon(Icon::empty().path("icons/arrow-down-to-line.svg"))
          .label("Next unviewed")
          .disabled(all_viewed)
          .on_click(cx.listener(|this, _, _, cx| this.next_unviewed(cx))),
      )
      .child(
        Button::new("all-changes-collapse")
          .ghost()
          .small()
          .icon(Icon::empty().path("icons/chevrons-down-up.svg"))
          .tooltip("Collapse all files")
          .accessibility_label("Collapse all files")
          .on_click(cx.listener(|this, _, _, cx| this.set_all_collapsed(true, cx))),
      )
      .child(
        Button::new("all-changes-expand")
          .ghost()
          .small()
          .icon(Icon::empty().path("icons/chevrons-up-down.svg"))
          .tooltip("Expand all files")
          .accessibility_label("Expand all files")
          .on_click(cx.listener(|this, _, _, cx| this.set_all_collapsed(false, cx))),
      )
      .child(
        h_flex()
          .gap_1p5()
          .items_center()
          .text_size(FONT_SIZE_SM)
          .text_color(if all_viewed { palette.new } else { muted })
          .child(ProgressCircle::new("viewed-progress").xsmall().value(100. * viewed as f32 / count.max(1) as f32))
          .child(format!("{viewed} / {count} viewed")),
      )
  }
}

fn reveal_file(state: &Entity<DiffState>, path: &str, cx: &mut App) {
  state.update(cx, |state, cx| {
    state.set_file_collapsed(path, false, cx);
    state.scroll_to_file(path, cx);
  });
}

impl Render for AllChanges {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    if let Some(error) = &self.error {
      return Label::new(error.clone()).p_4().text_color(cx.theme().danger).into_any_element();
    }
    if self.patch.as_deref() == Some("") {
      return Label::new("No changes to preview.").p_4().text_color(muted).into_any_element();
    }
    let Some(diff) = &self.diff else {
      return h_flex().gap_2().p_4().child(Spinner::new().small()).child(Label::new("Loading changes…").text_color(muted)).into_any_element();
    };
    let state = diff.state().clone();
    let view = cx.entity().downgrade();
    let layout = Rc::new(RefCell::new(FileHeaderLayout::default()));
    let header_layout = layout.clone();
    let line_layout = layout.clone();
    let header_view = view.clone();
    let diff = diff
      .view(cx)
      .soft_wrap(true)
      .text_size(code_font_size(&self.settings, cx))
      .border_0()
      .size_full()
      .gutter_markers(self.line_probes.iter().cloned())
      .render_gutter_marker(move |marker, _, _| {
        let layout = line_layout.clone();
        let path = marker.position().path().clone();
        canvas(
          move |bounds, _, _| {
            let mut layout = layout.borrow_mut();
            if layout.first_line.as_ref().is_none_or(|(_, top)| bounds.top() < *top) {
              layout.first_line = Some((path, bounds.top()));
            }
          },
          |_, _, _, _| {},
        )
        .size_full()
      })
      .render_header(move |file, _, cx| {
        let layout = header_layout.clone();
        let path = file.path().clone();
        file_header(file, &header_view, cx).relative().child(
          canvas(
            move |bounds, window, _| {
              // Measure the top of the Kit row, including its vertical padding and separator.
              let header_top = bounds.top() - window.rem_size() / 4.;
              let offset = header_top - window.content_mask().bounds.top();
              let mut layout = layout.borrow_mut();
              if offset < Pixels::ZERO {
                layout.above = Some(path);
              } else if layout.below.as_ref().is_none_or(|(_, top)| offset < *top) {
                layout.below = Some((path, offset));
              }
            },
            |_, _, _, _| {},
          )
          .absolute()
          .inset_0(),
        )
      });
    let current_file = self.current_file.clone();
    let sticky_state = state.clone();
    let sticky_header = canvas(
      move |viewport, window, cx| {
        let FileHeaderLayout { above, below, first_line } = std::mem::take(&mut *layout.borrow_mut());
        let files = sticky_state.read(cx).files();
        let mut current = current_file.borrow_mut();
        // When the next header reaches the top it can take over in the list itself.
        if let Some((path, _)) = below.as_ref().filter(|(_, offset)| *offset <= FILE_HEADER_SEPARATOR) {
          *current = Some(path.clone());
          return;
        }
        if let Some(path) = above {
          *current = Some(path);
        } else if let Some((next, _)) = &below {
          *current =
            files.iter().position(|file| file.path() == next).and_then(|index| index.checked_sub(1)).map(|index| files[index].path().clone());
        } else if let Some((path, _)) = first_line {
          *current = Some(path);
        }
        let Some(file) = current.as_ref().and_then(|path| files.iter().find(|file| file.path() == path)) else { return };
        let state = sticky_state.clone();
        let path = file.path().clone();
        let collapsed = state.read(cx).is_file_collapsed(&path);
        let mut header = h_flex()
          .id(SharedString::from(format!("sticky-diff-header-{path}")))
          .block_mouse_except_scroll()
          .w(viewport.size.width)
          .gap_1()
          .pl_1()
          .pr_3()
          .py_1()
          .border_b_1()
          .border_color(cx.theme().border)
          .bg(colors(cx).background)
          .child(
            Button::new("collapse-sticky-file")
              .ghost()
              .xsmall()
              .icon(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown })
              .accessibility_label(if collapsed { "Expand file" } else { "Collapse file" })
              .on_click(move |_, _, cx| {
                state.update(cx, |state, cx| state.set_file_collapsed(&path, !collapsed, cx));
              }),
          )
          .child(div().flex_1().min_w_0().child(file_header(file, &view, cx)))
          .into_any_element();
        let size = header.layout_as_root(size(viewport.size.width.into(), AvailableSpace::MinContent), window, cx);
        let offset = below.map_or(Pixels::ZERO, |(_, top)| (top - size.height).min(Pixels::ZERO));
        window.defer_draw(header, viewport.origin + point(Pixels::ZERO, offset), 0, Some(ContentMask { bounds: viewport }));
      },
      |_, _, _, _| {},
    )
    .absolute()
    .inset_0();
    v_flex()
      .size_full()
      .min_h_0()
      .child(self.toolbar(&state, cx))
      .child(h_flex().px_2().py_1().border_b_1().border_color(cx.theme().border).child(crate::diff_view::diff_controls(&state)))
      // The diff owns its scrolling, so its scrollbar sits at the right edge of the area.
      .child(div().relative().flex_1().min_h_0().overflow_hidden().cursor_text().child(diff).child(sticky_header))
      .into_any_element()
  }
}

/// A fingerprint of each file's part of the patch, to tell whether a reviewed file has changed since. The patch has one `diff --git`
/// section per file, in the same order as the parsed files; if they do not line up, nothing is fingerprinted.
fn file_fingerprints(patch: &str, files: &[DiffFile]) -> HashMap<SharedString, u64> {
  let mut sections: Vec<&str> = Vec::new();
  let mut start = None;
  let mut offset = 0;
  for line in patch.split_inclusive('\n') {
    if line.starts_with("diff --git ") {
      if let Some(from) = start {
        sections.push(&patch[from..offset]);
      }
      start = Some(offset);
    }
    offset += line.len();
  }
  if let Some(from) = start {
    sections.push(&patch[from..]);
  }
  if sections.len() != files.len() {
    return HashMap::new();
  }
  files
    .iter()
    .zip(sections)
    .map(|(file, section)| {
      let mut hasher = DefaultHasher::new();
      section.hash(&mut hasher);
      (file.path().clone(), hasher.finish())
    })
    .collect()
}

/// Sparse row probes identify the file after a scroll skips its original header.
fn line_probes(patch: &str, files: &[DiffFile]) -> Vec<DiffGutterMarker> {
  const PROBE_INTERVAL: usize = 8;
  let mut probes = Vec::new();
  visit_patch_lines(patch, files, |file, side, number, _, row| {
    if row.is_multiple_of(PROBE_INTERVAL) {
      let path = file.path();
      let id = SharedString::from(format!("sticky-probe-{path}-{side:?}-{number}"));
      probes.push(DiffGutterMarker::new(id, DiffLinePosition::new(path.clone(), side, number)));
    }
  });
  probes
}

/// The content of a file's header: its path with the open and copy buttons, the size of the change, and the review checkbox.
fn file_header(file: &DiffFile, view: &WeakEntity<AllChanges>, cx: &App) -> Div {
  let palette = colors(cx);
  let (added, removed) = (file.additions(), file.deletions());
  let total = added + removed;
  let blocks = total.min(5);
  let added_blocks = match (added, removed) {
    (0, _) => 0,
    (_, 0) => blocks,
    _ => ((added as f64 / total as f64 * blocks as f64).round() as usize).clamp(1, blocks - 1),
  };
  let path = file.path().clone();
  let checked = view.upgrade().is_some_and(|view| view.read(cx).viewed.contains_key(&path));
  let modified = file.modified_path().cloned();
  let title = match (file.original_path(), file.modified_path()) {
    (Some(old), Some(new)) if old != new => SharedString::from(format!("{old} → {new}")),
    _ => path.clone(),
  };
  let view = view.clone();
  h_flex()
    .w_full()
    .min_w_0()
    .gap_3()
    .child(
      h_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .child(file_title(&path, &title, cx))
        .when_some(modified, |row, modified| {
          row.child(
            div().cursor_pointer().child(
              Button::new(SharedString::from(format!("open-diff-file-{path}")))
                .ghost()
                .xsmall()
                .icon(Icon::empty().path("icons/code-xml.svg"))
                .tooltip("Open in Explorer")
                .accessibility_label("Open in Explorer")
                .on_click(move |_, window, cx| window.dispatch_action(Box::new(OpenChangedFile(modified.clone())), cx)),
            ),
          )
        })
        .child(
          div()
            .cursor_pointer()
            .child(Clipboard::new(SharedString::from(format!("copy-diff-path-{path}"))).value(path.clone()).tooltip("Copy relative path")),
        ),
    )
    .when(total > 0, |row| {
      row.child(
        h_flex()
          .flex_none()
          .gap_1()
          .child(div().text_xs().text_color(palette.new).child(format!("+{added}")))
          .child(div().text_xs().text_color(palette.deleted).child(format!("−{removed}")))
          .child(h_flex().gap_0p5().children((0..5).map(|index| {
            let color = if index < added_blocks {
              palette.new
            } else if index < blocks {
              palette.deleted
            } else {
              palette.border
            };
            div().size_1p5().rounded_sm().bg(color)
          }))),
      )
    })
    // The diff area shows a text cursor; the controls in the header are clickable.
    .child(div().cursor_pointer().child(Checkbox::new(SharedString::from(format!("diff-viewed-{path}"))).label("Viewed").checked(checked).on_click(
      move |checked, _, cx| {
        let _ = view.update(cx, |this, cx| {
          if *checked {
            let fingerprint = this.fingerprints.get(&path).copied().unwrap_or_default();
            this.viewed.insert(path.clone(), fingerprint);
          } else {
            this.viewed.remove(&path);
          }
          // A file that has been reviewed folds away; unchecking opens it again.
          if let Some(diff) = &this.diff {
            diff.state().update(cx, |state, cx| state.set_file_collapsed(&path, *checked, cx));
          }
          cx.notify();
        });
      },
    )))
}

/// The file's name in bold with its folder after it, soft: the name is what the eye looks for. A rename shows both paths.
fn file_title(path: &str, title: &str, cx: &App) -> impl IntoElement {
  let palette = colors(cx);
  let renamed = title != path;
  let (folder, name) = match path.rsplit_once('/') {
    Some((folder, name)) if !renamed => (Some(format!("{folder}/")), name),
    _ => (None, title),
  };
  h_flex()
    .min_w_0()
    .overflow_hidden()
    .whitespace_nowrap()
    .gap_0()
    .text_sm()
    .child(div().flex_none().font_semibold().child(name.to_string()))
    .children(folder.map(|folder| {
      div().ml_2().min_w_0().overflow_hidden().text_ellipsis().text_size(FONT_SIZE_SM).text_color(palette.text_secondary).child(folder)
    }))
}
