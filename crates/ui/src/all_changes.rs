//! The working tree's changes in one virtualized diff with per-file review controls.

use crate::{diff_view::ShownDiff, settings::code_font_size, theme::colors};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    clipboard::Clipboard,
    diff::{DiffFile, DiffGutterMarker, DiffLinePosition, DiffSide, DiffState},
    h_flex,
    label::Label,
    menu::{DropdownMenu as _, PopupMenuItem},
    progress::ProgressCircle,
    scroll::ScrollableElement as _,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
  },
  *,
};
use std::{cell::RefCell, collections::HashSet, path::PathBuf, rc::Rc, sync::Arc};

const HEADER_ALIGNMENT_TOLERANCE: Pixels = px(1.);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = vekro, no_json)]
pub(crate) struct OpenChangedFile(pub SharedString);

#[derive(Default)]
pub(crate) struct AllChanges {
  diff: Option<ShownDiff>,
  patch: Option<Arc<str>>,
  viewed: HashSet<SharedString>,
  line_markers: Vec<DiffGutterMarker>,
  compact_scroll: UniformListScrollHandle,
  compact: bool,
  diff_subscription: Option<Subscription>,
  error: Option<SharedString>,
  task: Option<Task<()>>,
}

impl AllChanges {
  pub fn reset(&mut self, cx: &mut Context<Self>) {
    *self = Self::default();
    cx.notify();
  }

  pub fn reload(&mut self, root: PathBuf, cx: &mut Context<Self>) {
    if self.task.is_some() {
      return;
    }
    let previous = self.patch.clone();
    let work = cx.background_executor().spawn(async move {
      let patch = vekro_git::all_changes_diff(&root)?;
      if previous.as_deref() == Some(patch.as_str()) {
        return Ok(None);
      }
      let files = DiffFile::parse(&patch).map_err(|error| error.to_string())?;
      let markers = line_markers(&patch, &files);
      Ok::<_, String>(Some((patch, files, markers)))
    });
    self.task = Some(cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        this.task = None;
        match result {
          Ok(None) => return,
          Ok(Some((patch, files, markers))) => {
            let paths: HashSet<_> = files.iter().map(DiffFile::path).collect();
            this.viewed.retain(|path| paths.contains(path));
            this.line_markers = markers;
            if let Some(diff) = &mut this.diff {
              diff.set_files(files, &patch, cx);
            } else {
              let diff = ShownDiff::new(files, &patch, cx);
              this.diff_subscription = Some(cx.observe(diff.state(), |this, state, cx| {
                let state = state.read(cx);
                let compact = state.files().iter().all(|file| state.is_file_collapsed(file.path()));
                if this.compact != compact {
                  this.compact = compact;
                  cx.notify();
                }
              }));
              this.diff = Some(diff);
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
    let files = diff.state().read(cx).files();
    let paths: Rc<[SharedString]> = files.iter().map(|file| file.path().clone()).collect();
    let count = paths.len();
    let viewed = self.viewed.len();
    let compact = self.compact;
    let compact_scroll = self.compact_scroll.clone();
    let menu_paths = paths.clone();
    let toolbar = h_flex()
      .justify_between()
      .gap_2()
      .child(
        Button::new("all-changes-files")
          .ghost()
          .small()
          .icon(Icon::empty().path("icons/files.svg"))
          .label("All changes")
          .child(Icon::new(IconName::ChevronDown).size_3())
          .dropdown_menu(move |menu, _, _| {
            menu_paths.iter().enumerate().fold(menu.scrollable(true), |menu, (index, path)| {
              let path = path.clone();
              let state = state.clone();
              let compact_scroll = compact_scroll.clone();
              menu.item(PopupMenuItem::new(path.clone()).on_click(move |_, _, cx| {
                if compact {
                  compact_scroll.scroll_to_item(index, ScrollStrategy::Nearest);
                }
                state.update(cx, |state, cx| state.scroll_to_file(&path, cx));
              }))
            })
          }),
      )
      .child(
        h_flex()
          .gap_1()
          .text_xs()
          .text_color(muted)
          .child(ProgressCircle::new("viewed-progress").xsmall().value(100. * viewed as f32 / count.max(1) as f32))
          .child(format!("{viewed} / {count} viewed")),
      );
    if compact {
      return v_flex().size_full().min_h_0().p_3().gap_2().child(toolbar).child(self.compact_list(count, cx)).into_any_element();
    }
    let rows = Rc::new(RefCell::new(Vec::new()));
    let header_positions = rows.clone();
    let first_path = paths.first().cloned();
    let line_positions = rows.clone();
    let header_view = cx.entity().downgrade();
    let review = cx.entity().downgrade();
    let diff = diff
      .view(cx)
      .soft_wrap(true)
      .text_size(code_font_size(cx))
      .border_0()
      .size_full()
      .gutter_markers(self.line_markers.iter().cloned())
      .render_gutter_marker(move |marker, _, _| row_probe(marker.position().path().clone(), false, line_positions.clone()).size_full())
      .render_header(move |file, _, cx| {
        let content = header_view.upgrade().map(|view| view.read(cx).file_header(file, header_view.clone(), false, cx));
        div()
          .relative()
          .w_full()
          .when(first_path.as_ref() != Some(file.path()), |header| header.pt_2())
          .children(content)
          .child(row_probe(file.path().clone(), true, header_positions.clone()).absolute().inset_0())
      });
    v_flex()
      .size_full()
      .min_h_0()
      .p_3()
      .gap_2()
      .child(toolbar)
      .child(
        div().relative().flex_1().min_h_0().overflow_hidden().child(diff).child(
          canvas(
            move |viewport, window, cx| {
              let positions = std::mem::take(&mut *rows.borrow_mut());
              let path = sticky_path(&positions, viewport.top(), window.rem_size())?;
              let owner = review.upgrade()?;
              let mut header = owner.read(cx).sticky_header(path, review.clone(), cx)?;
              header.prepaint_as_root(viewport.origin, size(viewport.size.width.into(), AvailableSpace::MinContent), window, cx);
              Some(header)
            },
            |_, header, window, cx| {
              if let Some(mut header) = header {
                header.paint(window, cx);
              }
            },
          )
          .absolute()
          .inset_0(),
        ),
      )
      .into_any_element()
  }
}

impl AllChanges {
  fn compact_list(&self, count: usize, cx: &mut Context<Self>) -> impl IntoElement {
    let handle = self.compact_scroll.clone();
    div()
      .relative()
      .min_h_0()
      .child(
        uniform_list(
          "collapsed-changes",
          count,
          cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
            let Some(diff) = &this.diff else { return Vec::new() };
            let state = diff.state();
            state
              .read(cx)
              .files()
              .get(range)
              .unwrap_or_default()
              .iter()
              .map(|file| div().pb_2().child(this.header_card(file, state, cx.entity().downgrade(), false, cx)).into_any_element())
              .collect()
          }),
        )
        .with_sizing_behavior(ListSizingBehavior::Infer)
        .w_full()
        .track_scroll(&handle),
      )
      .child(div().absolute().inset_0().vertical_scrollbar(&handle))
  }

  fn sticky_header(&self, path: SharedString, view: WeakEntity<Self>, cx: &App) -> Option<AnyElement> {
    let state = self.diff.as_ref()?.state();
    let file = state.read(cx).files().iter().find(|file| file.path() == &path)?;
    Some(self.header_card(file, state, view, true, cx).into_any_element())
  }

  fn header_card(&self, file: &DiffFile, state: &Entity<DiffState>, view: WeakEntity<Self>, sticky: bool, cx: &App) -> impl IntoElement + use<> {
    let path = file.path().clone();
    let collapsed = state.read(cx).is_file_collapsed(&path);
    let state = state.clone();
    let header = self.file_header(file, view, true, cx);
    h_flex()
      .id(SharedString::from(format!("review-file-{path}")))
      .when(sticky, |row| row.block_mouse_except_scroll().border_b_1().bg(cx.theme().background))
      .when(!sticky, |row| row.rounded_md().border_1().bg(colors(cx).surface_alt))
      .w_full()
      .pl_1()
      .pr_3()
      .py_1()
      .gap_1()
      .border_color(cx.theme().border)
      .child(
        Button::new("review-collapse")
          .ghost()
          .xsmall()
          .icon(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown })
          .accessibility_label(format!("{} {path}", if collapsed { "Expand" } else { "Collapse" }))
          .on_click(move |_, _, cx| {
            state.update(cx, |state, cx| {
              state.set_file_collapsed(&path, !collapsed, cx);
              state.scroll_to_file(&path, cx);
            });
          }),
      )
      .child(header)
  }

  fn file_header(&self, file: &DiffFile, view: WeakEntity<Self>, pinned: bool, cx: &App) -> impl IntoElement + use<> {
    let palette = colors(cx);
    let added = file.additions();
    let removed = file.deletions();
    let total = added + removed;
    let blocks = total.min(5);
    let added_blocks = match (added, removed) {
      (0, _) => 0,
      (_, 0) => blocks,
      _ => ((added as f64 / total as f64 * blocks as f64).round() as usize).clamp(1, blocks - 1),
    };
    let path = file.path().clone();
    let checked = self.viewed.contains(&path);
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
          .child(div().text_sm().min_w_0().truncate().child(title))
          .when_some(modified, |row, modified| {
            row.child(
              Button::new(SharedString::from(format!("open-diff-file-{path}")))
                .ghost()
                .xsmall()
                .icon(Icon::empty().path("icons/code-xml.svg"))
                .tooltip("Open in Explorer")
                .accessibility_label("Open in Explorer")
                .on_click(move |_, window, cx| {
                  window.dispatch_action(Box::new(OpenChangedFile(modified.clone())), cx);
                }),
            )
          })
          .child(Clipboard::new(SharedString::from(format!("copy-diff-path-{path}"))).value(path.clone()).tooltip("Copy relative path")),
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
      .child(Checkbox::new(SharedString::from(format!("diff-viewed-{path}"))).label("Viewed").checked(checked).on_click(move |checked, _, cx| {
        let _ = view.update(cx, |this, cx| {
          if *checked {
            this.viewed.insert(path.clone());
          } else {
            this.viewed.remove(&path);
          }
          if let Some(diff) = &this.diff {
            diff.state().update(cx, |state, cx| {
              state.set_file_collapsed(&path, *checked, cx);
              if pinned {
                state.scroll_to_file(&path, cx);
              }
            });
          }
          cx.notify();
        });
      }))
  }
}

// The Kit virtualizes the diff, so the header can be unmounted far above its code rows.
struct DiffRowPosition {
  path: SharedString,
  y: Pixels,
  header: bool,
}

fn row_probe(path: SharedString, header: bool, positions: Rc<RefCell<Vec<DiffRowPosition>>>) -> Canvas<()> {
  canvas(move |bounds, _, _| positions.borrow_mut().push(DiffRowPosition { path: path.clone(), y: bounds.top(), header }), |_, _, _, _| {})
}

fn sticky_path(rows: &[DiffRowPosition], top: Pixels, rem: Pixels) -> Option<SharedString> {
  let row_top = |row: &DiffRowPosition| if row.header { row.y - rem / 4. } else { row.y };
  let next = rows.iter().filter(|row| row_top(row) >= top).min_by_key(|row| row_top(row));
  if next.is_some_and(|row| row.header && row_top(row) - top <= HEADER_ALIGNMENT_TOLERANCE) {
    return None;
  }
  rows
    .iter()
    .filter(|row| row_top(row) < top)
    .max_by_key(|row| row_top(row))
    .or(next)
    .filter(|row| row_top(row) < top || !row.header)
    .map(|row| row.path.clone())
}

// Use the patch's numbered hunk ranges for scroll probes in the Kit's gutter lane.
fn line_markers(patch: &str, files: &[DiffFile]) -> Vec<DiffGutterMarker> {
  let mut markers = Vec::new();
  let mut paths = files.iter();
  let mut path = None;
  for line in patch.lines() {
    if line.starts_with("diff --git ") {
      path = paths.next().map(DiffFile::path);
    } else if let Some(header) = line.strip_prefix("@@ ") {
      let Some(path) = path else { continue };
      for (side, range) in [DiffSide::Original, DiffSide::Modified].into_iter().zip(header.split_whitespace().take(2)) {
        let range = range.trim_start_matches(['-', '+']);
        let (start, count) = range.split_once(',').unwrap_or((range, "1"));
        let (Ok(start), Ok(count)) = (start.parse::<usize>(), count.parse::<usize>()) else { continue };
        markers.extend(
          (start..start.saturating_add(count))
            .map(|line| DiffGutterMarker::new(("diff-scroll-probe", line), DiffLinePosition::new(path.clone(), side, line))),
        );
      }
    }
  }
  markers
}
