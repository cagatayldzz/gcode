//! The working tree's changes in one virtualized diff with per-file review controls.

use crate::{diff_view::ShownDiff, settings::code_font_size, theme::colors};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    clipboard::Clipboard,
    diff::{DiffAnnotation, DiffFile, DiffGutterMarker, DiffLinePosition, DiffSide, DiffState},
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

const CARD_BORDER_WIDTH: Pixels = px(1.);
const CARD_GAP: Rems = rems(0.75);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = vekro, no_json)]
pub(crate) struct OpenChangedFile(pub SharedString);

#[derive(Default)]
pub(crate) struct AllChanges {
  diff: Option<ShownDiff>,
  patch: Option<Arc<str>>,
  viewed: HashSet<SharedString>,
  line_markers: Vec<DiffGutterMarker>,
  last_line: Option<DiffLinePosition>,
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
      let (markers, last_line) = line_markers(&patch, &files);
      Ok::<_, String>(Some((patch, files, markers, last_line)))
    });
    self.task = Some(cx.spawn(async move |this, cx| {
      let result = work.await;
      let _ = this.update(cx, |this, cx| {
        this.task = None;
        match result {
          Ok(None) => return,
          Ok(Some((patch, files, markers, last_line))) => {
            let paths: HashSet<_> = files.iter().map(DiffFile::path).collect();
            this.viewed.retain(|path| paths.contains(path));
            this.line_markers = markers;
            this.last_line = last_line;
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
    let menu_state = state.clone();
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
              let state = menu_state.clone();
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
    let annotations = files
      .iter()
      .map(|file| DiffAnnotation::file(file.path().clone(), file.path().clone()))
      .chain(self.last_line.clone().map(|position| DiffAnnotation::line("review-end", position)));
    let headers: Rc<_> = files
      .iter()
      .map(|file| (file.path().clone(), (file.clone(), state.read(cx).is_file_collapsed(file.path()))))
      .collect::<std::collections::HashMap<_, _>>()
      .into();
    let first_path = paths.first().cloned();
    let last_path = paths.last().cloned();
    let last_notice = self.last_line.is_none().then(|| files.last().cloned()).flatten();
    let header_positions = rows.clone();
    let line_positions = rows.clone();
    let header_view = cx.entity().downgrade();
    let review = cx.entity().downgrade();
    let (background, border) = (cx.theme().background, cx.theme().border);
    let diff = diff
      .view(cx)
      .soft_wrap(true)
      .text_size(code_font_size(cx))
      .border_0()
      .size_full()
      .header_visible(false)
      .annotations(annotations)
      .gutter_markers(self.line_markers.iter().cloned())
      .render_gutter_marker(move |marker, _, _| row_probe(marker.position().path().clone(), RowKind::Code, line_positions.clone()).size_full())
      .render_annotation(move |annotation, window, cx| {
        // Kit annotations normally have their own padded surface. The file card supplies that surface here.
        let padding = window.rem_size() * 0.75;
        let margin = -(padding + CARD_BORDER_WIDTH);
        let path = annotation.path().clone();
        let wrapper = v_flex().mx(-padding).mt(margin).mb(margin);
        if annotation.position().is_some() {
          return wrapper.child(row_probe(path, RowKind::End, header_positions.clone()).h_0()).into_any_element();
        }
        let Some((file, collapsed)) = headers.get(&path) else { return wrapper.into_any_element() };
        let Some(owner) = header_view.upgrade() else { return wrapper.into_any_element() };
        wrapper
          .when(first_path.as_ref() != Some(&path), |this| this.child(div().h(CARD_GAP).bg(background)))
          .child(
            div()
              .relative()
              .child(owner.read(cx).header_card(file, &state, *collapsed, header_view.clone(), false, cx))
              .child(row_probe(path, RowKind::Header { collapsed: *collapsed }, header_positions.clone()).absolute().inset_0()),
          )
          .into_any_element()
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
              let pinned = sticky_path(&positions, viewport.top());
              let frames = card_frames(&positions, viewport, pinned.as_ref(), last_path.as_ref(), last_notice.as_ref(), window, cx);
              let header = pinned.and_then(|path| {
                let owner = review.upgrade()?;
                let mut header = owner.read(cx).sticky_header(path, review.clone(), cx)?;
                header.prepaint_as_root(viewport.origin, size(viewport.size.width.into(), AvailableSpace::MinContent), window, cx);
                Some(header)
              });
              let tail = positions
                .iter()
                .find(|row| matches!(row.kind, RowKind::End))
                .map(|row| row.bounds.top())
                .filter(|end| viewport.top() < *end && *end < viewport.bottom());
              (frames, header, tail, cx.theme().radius)
            },
            move |viewport, (frames, header, tail, radius), window, cx| {
              window.with_content_mask(Some(ContentMask { bounds: viewport }), |window| {
                // The measuring annotation retains the Kit's surface padding after the last source row.
                if let Some(top) = tail {
                  window.paint_quad(fill(Bounds::from_corners(point(viewport.left(), top), point(viewport.right(), viewport.bottom())), background));
                }
                if let Some(mut header) = header {
                  header.paint(window, cx);
                }
                for bounds in frames {
                  window.paint_quad(quad(bounds, radius, transparent_black(), CARD_BORDER_WIDTH, border, BorderStyle::Solid));
                }
              });
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
              .map(|file| div().pb_2().child(this.header_card(file, state, true, cx.entity().downgrade(), true, cx)).into_any_element())
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
    let collapsed = state.read(cx).is_file_collapsed(&path);
    Some(self.header_card(file, state, collapsed, view, true, cx).into_any_element())
  }

  fn header_card(
    &self,
    file: &DiffFile,
    state: &Entity<DiffState>,
    collapsed: bool,
    view: WeakEntity<Self>,
    standalone: bool,
    cx: &App,
  ) -> impl IntoElement + use<> {
    let path = file.path().clone();
    let state = state.clone();
    let header = self.file_header(file, view.clone(), standalone, cx);
    h_flex()
      .id(SharedString::from(format!("review-file-{path}")))
      .bg(colors(cx).surface_alt)
      .when(standalone, |row| row.block_mouse_except_scroll())
      .when(collapsed, |row| row.rounded_md())
      .when(collapsed && standalone, |row| row.border_1())
      .when(!collapsed, |row| row.rounded_t_md().border_b_1())
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
              if standalone {
                state.scroll_to_file(&path, cx);
              }
            });
            let _ = view.update(cx, |_, cx| cx.notify());
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

#[derive(Clone, Copy)]
enum RowKind {
  Header { collapsed: bool },
  Code,
  End,
}

// The Kit virtualizes the diff, so a file header can be unmounted while its code is visible.
struct DiffRowPosition {
  path: SharedString,
  bounds: Bounds<Pixels>,
  kind: RowKind,
}

fn row_probe(path: SharedString, kind: RowKind, positions: Rc<RefCell<Vec<DiffRowPosition>>>) -> Canvas<()> {
  canvas(move |bounds, _, _| positions.borrow_mut().push(DiffRowPosition { path: path.clone(), bounds, kind }), |_, _, _, _| {})
}

fn sticky_path(rows: &[DiffRowPosition], top: Pixels) -> Option<SharedString> {
  let next = rows.iter().filter(|row| row.bounds.top() >= top).min_by_key(|row| row.bounds.top());
  if next.is_some_and(|row| matches!(row.kind, RowKind::Header { .. }) && row.bounds.top() - top <= CARD_BORDER_WIDTH) {
    return None;
  }
  rows
    .iter()
    .filter(|row| row.bounds.top() < top)
    .max_by_key(|row| row.bounds.top())
    .or(next)
    .filter(|row| !matches!(row.kind, RowKind::End | RowKind::Header { collapsed: true }))
    .filter(|row| row.bounds.top() < top || matches!(row.kind, RowKind::Code))
    .map(|row| row.path.clone())
}

fn card_frames(
  rows: &[DiffRowPosition],
  viewport: Bounds<Pixels>,
  pinned: Option<&SharedString>,
  last: Option<&SharedString>,
  last_notice: Option<&DiffFile>,
  window: &mut Window,
  cx: &mut App,
) -> Vec<Bounds<Pixels>> {
  let mut headers: Vec<_> = rows.iter().filter(|row| matches!(row.kind, RowKind::Header { .. })).collect();
  headers.sort_by_key(|row| row.bounds.top());
  let rem = window.rem_size();
  let gap = rem * CARD_GAP.0;
  let end = rows.iter().find(|row| matches!(row.kind, RowKind::End)).map(|row| row.bounds.top());
  let below = viewport.bottom() + rem;
  let frame = |top, bottom| Bounds::from_corners(point(viewport.left(), top), point(viewport.right(), bottom));
  let mut frames = Vec::with_capacity(headers.len() + 1);
  if pinned.is_some_and(|path| !headers.iter().any(|header| &header.path == path)) {
    let bottom = headers.first().map(|header| header.bounds.top() - gap).or(end).unwrap_or(below);
    frames.push(frame(viewport.top() - rem, bottom));
  }
  for (index, header) in headers.iter().enumerate() {
    let bottom = if matches!(header.kind, RowKind::Header { collapsed: true }) {
      header.bounds.bottom()
    } else if let Some(next) = headers.get(index + 1) {
      next.bounds.top() - gap
    } else if Some(&header.path) == last {
      if let Some(file) = last_notice {
        header.bounds.bottom() + notice_height(file, viewport.size.width, window, cx)
      } else {
        end.unwrap_or(below)
      }
    } else {
      below
    };
    if bottom > header.bounds.top() {
      frames.push(frame(header.bounds.top(), bottom));
    }
  }
  frames
}

// Metadata-only files end with the Kit's notice rather than a source row.
fn notice_height(file: &DiffFile, width: Pixels, window: &mut Window, cx: &mut App) -> Pixels {
  v_flex()
    .w_full()
    .px_3()
    .py_2()
    .gap_1()
    .text_xs()
    .children(file.extended_headers().iter().cloned().map(|line| div().child(line)))
    .child(div().text_sm().child("No text changes"))
    .into_any_element()
    .layout_as_root(size(width.into(), AvailableSpace::MinContent), window, cx)
    .height
}

// Use the patch's numbered hunk ranges for scroll probes in the Kit's gutter lane.
fn line_markers(patch: &str, files: &[DiffFile]) -> (Vec<DiffGutterMarker>, Option<DiffLinePosition>) {
  let mut markers = Vec::new();
  let mut paths = files.iter();
  let mut path = None;
  let mut last_line = None;
  let mut numbers = None;
  for line in patch.lines() {
    if line.starts_with("diff --git ") {
      path = paths.next().map(DiffFile::path);
      last_line = None;
      numbers = None;
    } else if let Some(header) = line.strip_prefix("@@ ") {
      let Some(path) = path else { continue };
      let mut starts = [0; 2];
      for (index, (side, range)) in [DiffSide::Original, DiffSide::Modified].into_iter().zip(header.split_whitespace().take(2)).enumerate() {
        let range = range.trim_start_matches(['-', '+']);
        let (start, count) = range.split_once(',').unwrap_or((range, "1"));
        let (Ok(start), Ok(count)) = (start.parse::<usize>(), count.parse::<usize>()) else { continue };
        starts[index] = start;
        markers.extend(
          (start..start.saturating_add(count))
            .map(|line| DiffGutterMarker::new(("diff-scroll-probe", line), DiffLinePosition::new(path.clone(), side, line))),
        );
      }
      numbers = Some(starts);
    } else if let (Some(path), Some([old, new])) = (path, numbers.as_mut()) {
      match line.as_bytes().first() {
        Some(b' ' | b'+') => {
          last_line = Some(DiffLinePosition::new(path.clone(), DiffSide::Modified, *new));
          *new += 1;
          *old += usize::from(line.starts_with(' '));
        }
        Some(b'-') => {
          last_line = Some(DiffLinePosition::new(path.clone(), DiffSide::Original, *old));
          *old += 1;
        }
        _ => {}
      }
    }
  }
  (markers, last_line)
}
