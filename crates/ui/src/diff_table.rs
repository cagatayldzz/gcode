//! The diff of one file as the Changes and History views show it, drawn here instead of with the Kit's `Diff`: GitHub Desktop's look
//! with a rule beside each line-number column, a band of folded lines with expand arrows in the number columns, and the words that
//! changed (never the indentation) emphasized. The lines come from [`crate::diff_model`]; this file is only the drawing.

use crate::{
  diff_model::{self, Edge, Expansion, Layout, LineKind, Patch, Row},
  diff_view::{highlight_theme, set_wraps, styles_of, wraps},
  editor::language_for,
  settings::{code_font_size, SettingsModel},
  theme::{colors, FONT_SIZE_SM},
  CopySelection,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    highlighter::HighlightTheme,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    scroll::{ScrollableMask, Scrollbar},
    tooltip::Tooltip,
    ActiveTheme as _, Icon, IconName, Sizable as _, Theme,
  },
  *,
};
use std::{ops::Range, path::Path, sync::Arc};

/// How wide one character of the code font is, as a share of its size, for the width of the line-number columns.
const MONO_ADVANCE: f32 = 0.62;

/// What a line-number column has beside its digits.
const NUMBER_PADDING: Pixels = px(28.);

/// The column with the `+` or `−` sign, and the room around the code, for the width of a line that is not wrapped.
const SIGN_WIDTH: Pixels = px(20.);
const CODE_PADDING: Pixels = px(16.);

/// How tall a line is, as a multiple of the code font size.
const LINE_HEIGHT: f32 = 1.6;

/// Syntax colors for the lines of a patch, one list for each line.
type LineStyles = Vec<Vec<(Range<usize>, HighlightStyle)>>;

/// A patch read and colored, ready to be shown; made on a background thread, since coloring a long file takes a while.
pub(crate) struct PreparedDiff {
  patch: Patch,
  language: &'static str,
  styles: LineStyles,
}

/// Reads `patch`, the diff of `file`, and colors its lines. Fails when the patch has no text lines to show.
pub(crate) fn prepare(theme: &Arc<HighlightTheme>, patch: &str, file: &Path) -> Result<PreparedDiff, String> {
  let patch = Patch::parse(patch);
  if patch.lines.is_empty() {
    return Err("This file has no text changes to show.".into());
  }
  let language = language_for(file);
  let styles = color(theme, &patch, language);
  Ok(PreparedDiff { patch, language, styles })
}

/// The syntax colors of every line of `patch`, in the language the editor uses for the file.
fn color(theme: &Arc<HighlightTheme>, patch: &Patch, language: &str) -> LineStyles {
  let mut styles: LineStyles = vec![Vec::new(); patch.lines.len()];
  if language == "text" {
    return styles;
  }
  for hunk in patch.hunks() {
    // The two sides are read the way `diff_view::warm` reads them, so that a hunk it already colored is found in the cache.
    let sides = [(true, patch.side_text(hunk.clone(), true)), (false, patch.side_text(hunk.clone(), false))];
    for (removed_side, (text, starts)) in sides {
      let colors = styles_of(theme, language, &text);
      for (offset, start) in starts.iter().enumerate() {
        let ix = hunk.start + offset;
        let line = &patch.lines[ix];
        // A context line is colored by the new side; each changed line by the side it is on.
        let counts = match line.kind {
          LineKind::Context => !removed_side,
          LineKind::Removed => removed_side,
          LineKind::Added => !removed_side,
        };
        if let (Some(start), true) = (start, counts) {
          styles[ix] = runs_within(&colors, *start..*start + line.text.len());
        }
      }
    }
  }
  styles
}

/// The runs of `styles` (sorted, not overlapping) that fall in `line`, as ranges of the line's own text.
fn runs_within(styles: &[(Range<usize>, HighlightStyle)], line: Range<usize>) -> Vec<(Range<usize>, HighlightStyle)> {
  let first = styles.partition_point(|(range, _)| range.end <= line.start);
  styles[first..]
    .iter()
    .take_while(|(range, _)| range.start < line.end)
    .map(|(range, style)| (range.start.max(line.start) - line.start..range.end.min(line.end) - line.start, *style))
    .collect()
}

/// The syntax runs of a line with background `overlays` laid over them (a later one wins where they cross): every run is cut where an
/// overlay starts or ends, and the pieces inside one get its background.
fn with_overlays(syntax: &[(Range<usize>, HighlightStyle)], overlays: &[(Range<usize>, Hsla)], length: usize) -> Vec<(Range<usize>, HighlightStyle)> {
  if overlays.is_empty() {
    return syntax.to_vec();
  }
  let mut cuts: Vec<usize> = vec![0, length];
  for (range, _) in syntax {
    cuts.extend([range.start, range.end]);
  }
  for (range, _) in overlays {
    cuts.extend([range.start, range.end]);
  }
  cuts.retain(|cut| *cut <= length);
  cuts.sort_unstable();
  cuts.dedup();
  cuts
    .windows(2)
    .filter_map(|pair| {
      let (start, end) = (pair[0], pair[1]);
      let mut style = syntax.iter().find(|(range, _)| range.start <= start && end <= range.end).map(|(_, style)| *style).unwrap_or_default();
      if let Some((_, background)) = overlays.iter().rev().find(|(range, _)| range.start <= start && end <= range.end) {
        style.background_color = Some(*background);
      }
      (style != HighlightStyle::default()).then_some((start..end, style))
    })
    .collect()
}

/// What is emphasized within a changed line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Emphasis {
  Words,
  Characters,
  Lines,
}

/// A place in the text of the patch: a line and a byte of it.
type Spot = (usize, usize);

/// A diff table: the lines of one file in a virtual list.
pub(crate) struct DiffTable {
  settings: Entity<SettingsModel>,
  patch: Patch,
  /// Whether any line has a number in the old file, and in the new one. A file that was added or deleted whole has one column only.
  has_old: bool,
  has_new: bool,
  styles: LineStyles,
  /// How much of each fold was brought back with the expand arrows.
  expansion: Expansion,
  layout: Layout,
  list: ListState,
  /// Old and new side by side instead of one column.
  split: bool,
  /// Whether long lines wrap, as the rows were last laid out; they are laid out again when the setting changes.
  wrap: bool,
  emphasis: Emphasis,
  /// The language the lines are colored in, and the work that colors them again when the theme changes.
  language: &'static str,
  recoloring: Option<Task<()>>,
  _theme_subscription: Subscription,
  /// The longest line, in characters, for the width of the table when lines do not wrap.
  widest: usize,
  scroll_x: ScrollHandle,
  focus: FocusHandle,
  /// The lines chosen by clicking or dragging on their numbers: where the choice began and where it is now, as indexes of the lines.
  selection: Option<(usize, usize)>,
  dragging: bool,
  /// Text chosen by dragging over it: where the choice began and where it is now.
  text_selection: Option<(Spot, Spot)>,
  selecting_text: bool,
}

/// A double click on a line: its line number in the new version of the file, to open the file there.
pub(crate) struct OpenLine(pub u32);

impl EventEmitter<OpenLine> for DiffTable {}

impl DiffTable {
  /// Asks to open the file at the line `ix` is on. A removed line has none of its own: the next line that is still there stands for it.
  fn open_line(&mut self, ix: usize, cx: &mut Context<Self>) {
    let lines = &self.patch.lines;
    let line = lines[ix..].iter().find_map(|line| line.new).or_else(|| lines[..ix].iter().rev().find_map(|line| line.new));
    if let Some(line) = line {
      cx.emit(OpenLine(line));
    }
  }

  pub fn new(prepared: PreparedDiff, settings: Entity<SettingsModel>, cx: &mut Context<Self>) -> Self {
    let PreparedDiff { patch, language, styles } = prepared;
    let expansion = Expansion::new();
    let layout = diff_model::layout(&patch, &expansion, false);
    let list = ListState::new(layout.rows.len(), ListAlignment::Top, px(600.));
    let widest = patch.lines.iter().map(|line| line.text.chars().count()).max().unwrap_or(0);
    let has_old = patch.lines.iter().any(|line| line.old.is_some());
    let has_new = patch.lines.iter().any(|line| line.new.is_some());
    Self {
      settings,
      patch,
      has_old,
      has_new,
      styles,
      expansion,
      layout,
      list,
      split: false,
      wrap: wraps(cx),
      emphasis: Emphasis::Words,
      language,
      recoloring: None,
      _theme_subscription: cx.observe_global::<Theme>(|this, cx| this.recolor(cx)),
      widest,
      scroll_x: ScrollHandle::new(),
      focus: cx.focus_handle(),
      selection: None,
      dragging: false,
      text_selection: None,
      selecting_text: false,
    }
  }

  /// Colors the lines again for the theme that was just chosen, on a background thread.
  fn recolor(&mut self, cx: &mut Context<Self>) {
    let (theme, patch, language) = (highlight_theme(cx), self.patch.clone(), self.language);
    let work = cx.background_executor().spawn(async move { color(&theme, &patch, language) });
    self.recoloring = Some(cx.spawn(async move |this, cx| {
      let styles = work.await;
      let _ = this.update(cx, |this, cx| {
        this.styles = styles;
        this.recoloring = None;
        cx.notify();
      });
    }));
  }

  /// Lays the rows out again after a fold was expanded, keeping the place in the list.
  fn relayout(&mut self) {
    let top = self.list.logical_scroll_top();
    self.layout = diff_model::layout(&self.patch, &self.expansion, self.split);
    self.list.reset(self.layout.rows.len());
    self.list.scroll_to(ListOffset { item_ix: top.item_ix.min(self.layout.rows.len().saturating_sub(1)), offset_in_item: top.offset_in_item });
  }

  fn expand(&mut self, base: usize, hidden: &Range<usize>, up: bool, cx: &mut Context<Self>) {
    diff_model::expand(&mut self.expansion, base, hidden, up);
    self.relayout();
    cx.notify();
  }

  /// Scrolls to the next change below the top of the list, or the previous one above it.
  pub fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
    let top = self.list.logical_scroll_top().item_ix;
    let target = if forward {
      self.layout.change_starts.iter().copied().find(|start| *start > top)
    } else {
      self.layout.change_starts.iter().rev().copied().find(|start| *start < top)
    };
    if let Some(item_ix) = target {
      self.list.scroll_to(ListOffset { item_ix, offset_in_item: px(0.) });
      cx.notify();
    }
  }

  fn select(&mut self, line: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
    let anchor = match (extend, self.selection) {
      (true, Some((anchor, _))) => anchor,
      _ => line,
    };
    self.selection = Some((anchor, line));
    self.text_selection = None;
    self.dragging = true;
    window.focus(&self.focus, cx);
    cx.notify();
  }

  fn drag_to(&mut self, line: usize, cx: &mut Context<Self>) {
    if let (true, Some((anchor, head))) = (self.dragging, self.selection) {
      if head != line {
        self.selection = Some((anchor, line));
        cx.notify();
      }
    }
  }

  /// A press on the code of `line`, at byte `at`: the start of a text selection.
  fn begin_text(&mut self, line: usize, at: usize, window: &mut Window, cx: &mut Context<Self>) {
    self.text_selection = Some(((line, at), (line, at)));
    self.selection = None;
    self.selecting_text = true;
    window.focus(&self.focus, cx);
    cx.notify();
  }

  fn extend_text(&mut self, line: usize, at: usize, cx: &mut Context<Self>) {
    if let (true, Some((anchor, head))) = (self.selecting_text, self.text_selection) {
      if head != (line, at) {
        self.text_selection = Some((anchor, (line, at)));
        cx.notify();
      }
    }
  }

  fn is_selected(&self, line: usize) -> bool {
    self.selection.is_some_and(|(anchor, head)| (anchor.min(head)..=anchor.max(head)).contains(&line))
  }

  /// The chosen part of the text of `line`, as a byte range.
  fn selected_in(&self, line: usize) -> Option<Range<usize>> {
    let (anchor, head) = self.text_selection?;
    let (start, end) = if anchor <= head { (anchor, head) } else { (head, anchor) };
    if line < start.0 || line > end.0 {
      return None;
    }
    let length = self.patch.lines[line].text.len();
    let range = if line == start.0 { start.1.min(length) } else { 0 }..if line == end.0 { end.1.min(length) } else { length };
    (range.start < range.end).then_some(range)
  }

  /// The text that Copy takes: the text chosen by dragging, or the lines chosen by their numbers; `line` alone when neither is
  /// chosen at it (from a right click).
  fn copy_text(&self, line: Option<usize>) -> Option<String> {
    if let Some((anchor, head)) = self.text_selection {
      let (start, end) = if anchor <= head { (anchor, head) } else { (head, anchor) };
      let parts: Vec<&str> = (start.0..=end.0).filter_map(|ix| self.selected_in(ix).map(|range| &self.patch.lines[ix].text[range])).collect();
      if !parts.is_empty() {
        return Some(parts.join("\n"));
      }
    }
    let range = match (self.selection, line) {
      (Some((anchor, head)), Some(line)) if self.is_selected(line) => anchor.min(head)..=anchor.max(head),
      (Some((anchor, head)), None) => anchor.min(head)..=anchor.max(head),
      (_, Some(line)) => line..=line,
      _ => return None,
    };
    Some(self.patch.lines[range].iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n"))
  }

  fn copy(&mut self, cx: &mut Context<Self>) {
    if let Some(text) = self.copy_text(None) {
      cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
  }

  fn set_split(&mut self, split: bool, cx: &mut Context<Self>) {
    if self.split != split {
      self.split = split;
      self.relayout();
      cx.notify();
    }
  }

  fn set_emphasis(&mut self, emphasis: Emphasis, cx: &mut Context<Self>) {
    self.emphasis = emphasis;
    cx.notify();
  }

  fn row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
    let size = code_font_size(&self.settings, cx);
    match self.layout.rows[ix].clone() {
      Row::Line(line) => div().w_full().flex().child(self.cell(line, Numbers::Both, size, cx)).into_any_element(),
      Row::Pair(old, new) => {
        let palette = colors(cx);
        let side = |line: Option<usize>, numbers: Numbers, cx: &mut Context<Self>| match line {
          Some(line) => self.cell(line, numbers, size, cx),
          None => div()
            .id(("diff-empty", ix * 2 + usize::from(numbers == Numbers::New)))
            .flex_1()
            .min_w_0()
            .self_stretch()
            .bg(palette.surface)
            .border_r_1()
            .border_color(palette.border)
            .into_any_element(),
        };
        h_flex()
          .id(("diff-pair", ix))
          .w_full()
          .items_stretch()
          .child(side(old, Numbers::Old, cx))
          .child(div().w_px().flex_none().self_stretch().bg(palette.border))
          .child(side(new, Numbers::New, cx))
          .into_any_element()
      }
      Row::Fold { hidden, base, edge, label } => self.fold_row(hidden, base, edge, label, size, cx),
    }
  }

  /// The width of one line-number column.
  fn number_width(&self, size: Pixels) -> Pixels {
    px(self.patch.digits() as f32 * MONO_ADVANCE * f32::from(size)) + NUMBER_PADDING
  }

  /// The width of the table when lines do not wrap: the longest line with the columns around it, twice in side by side.
  fn content_width(&self, size: Pixels) -> Pixels {
    let numbers = self.number_width(size);
    let code = px(self.widest as f32 * MONO_ADVANCE * f32::from(size)) + CODE_PADDING + SIGN_WIDTH;
    if self.split {
      (numbers + code) * 2. + px(1.)
    } else {
      numbers * (usize::from(self.has_old) + usize::from(self.has_new)).max(1) as f32 + code
    }
  }

  /// One line with its number columns, sign and code: the whole row in one column, one half of it side by side.
  fn cell(&self, ix: usize, numbers: Numbers, size: Pixels, cx: &mut Context<Self>) -> AnyElement {
    let palette = colors(cx);
    let theme = cx.theme().clone();
    let line = &self.patch.lines[ix];
    let selected = self.is_selected(ix);
    let (background, number_background, word) = match (selected, line.kind) {
      (true, _) => (palette.accent.opacity(0.2), palette.accent, palette.accent.opacity(0.0)),
      (_, LineKind::Added) => (palette.diff_added, palette.diff_added_gutter, palette.diff_added_word),
      (_, LineKind::Removed) => (palette.diff_removed, palette.diff_removed_gutter, palette.diff_removed_word),
      (_, LineKind::Context) => (Hsla::transparent_black(), Hsla::transparent_black(), Hsla::transparent_black()),
    };
    let number_color = if selected { palette.on_accent } else { palette.text_secondary };
    let width = self.number_width(size);
    let number = |value: Option<u32>, id: &'static str| {
      div()
        .id((id, ix))
        .w(width)
        .flex_none()
        .px_2()
        .text_right()
        .whitespace_nowrap()
        .text_color(number_color)
        .bg(number_background)
        .border_r_1()
        .border_color(palette.border)
        .cursor_pointer()
        .child(value.map(|value| value.to_string()).unwrap_or_default())
        .on_mouse_down(
          MouseButton::Left,
          cx.listener(move |this, event: &MouseDownEvent, window, cx| {
            if event.click_count >= 2 {
              this.open_line(ix, cx);
            }
            this.select(ix, event.modifiers.shift, window, cx)
          }),
        )
    };
    let sign = match line.kind {
      LineKind::Added => "+",
      LineKind::Removed => "−",
      LineKind::Context => "",
    };
    let text: SharedString = if line.text.is_empty() { " ".into() } else { line.text.clone().into() };
    let mut overlays: Vec<(Range<usize>, Hsla)> = Vec::new();
    match self.emphasis {
      Emphasis::Words => overlays.extend(self.patch.emphasis[ix].iter().map(|range| (range.clone(), word))),
      Emphasis::Characters => overlays.extend(self.patch.char_emphasis[ix].iter().map(|range| (range.clone(), word))),
      Emphasis::Lines if !matches!(line.kind, LineKind::Context) => overlays.push((0..line.text.len(), word)),
      Emphasis::Lines => {}
    }
    overlays.extend(self.selected_in(ix).map(|range| (range, palette.accent.opacity(0.35))));
    let highlights = with_overlays(&self.styles[ix], &overlays, line.text.len());
    let styled = StyledText::new(text).with_highlights(highlights);
    let layout = styled.layout().clone();
    let at = |layout: &TextLayout, position: Point<Pixels>| layout.index_for_position(position).unwrap_or_else(|nearest| nearest);
    let table = cx.entity();
    let wrap = self.wrap;
    div()
      .id(("diff-cell", ix * 3 + numbers as usize))
      .flex()
      .flex_row()
      .flex_1()
      .min_w_0()
      .items_start()
      .min_h(size * LINE_HEIGHT)
      .bg(background)
      .font_family(theme.mono_font_family.clone())
      .text_size(size)
      .line_height(size * LINE_HEIGHT)
      .when(numbers != Numbers::New && (numbers == Numbers::Old || self.has_old), |cell| cell.child(number(line.old, "old-number")))
      .when(numbers != Numbers::Old && (numbers == Numbers::New || self.has_new), |cell| cell.child(number(line.new, "new-number")))
      .child(div().w(SIGN_WIDTH).flex_none().text_center().text_color(palette.text_secondary).child(sign))
      .child(
        div()
          .id(("diff-code", ix))
          .flex_1()
          .min_w_0()
          .pr_2()
          .cursor_text()
          .when(wrap, |code| code.whitespace_normal())
          .when(!wrap, |code| code.whitespace_nowrap())
          .child(styled)
          .on_mouse_down(MouseButton::Left, {
            let layout = layout.clone();
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
              if event.click_count >= 2 {
                this.open_line(ix, cx);
              }
              this.begin_text(ix, at(&layout, event.position), window, cx)
            })
          }),
      )
      .on_mouse_move({
        let layout = layout.clone();
        cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
          this.drag_to(ix, cx);
          this.extend_text(ix, at(&layout, event.position), cx);
        })
      })
      .context_menu(move |menu, _, _| {
        let table = table.clone();
        menu.item(PopupMenuItem::new("Copy").icon(Icon::empty().path("icons/copy.svg")).on_click(move |_, _, cx| {
          if let Some(text) = table.read(cx).copy_text(Some(ix)) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
          }
        }))
      })
      .into_any_element()
  }

  fn fold_row(&self, hidden: Range<usize>, base: usize, edge: Edge, label: String, size: Pixels, cx: &mut Context<Self>) -> AnyElement {
    let palette = colors(cx);
    let theme = cx.theme().clone();
    let width = self.number_width(size);
    // `up` brings back the lines next to the ones below the fold, `down` those next to the ones above it.
    let arrow = |up: bool| {
      let hidden = hidden.clone();
      div()
        .id((if up { "expand-up" } else { "expand-down" }, base))
        .flex_none()
        .cursor_pointer()
        .text_color(palette.diff_band_text)
        .child(Icon::empty().path(if up { "icons/arrow-up.svg" } else { "icons/arrow-down.svg" }).size_4())
        .tooltip({
          let text = if up { "Expand Up" } else { "Expand Down" };
          move |window, cx| Tooltip::new(text).build(window, cx)
        })
        .on_click(cx.listener(move |this, _, _, cx| this.expand(base, &hidden, up, cx)))
    };
    h_flex()
      .id(("diff-fold", base))
      .w_full()
      .h(size * LINE_HEIGHT)
      .items_center()
      .bg(palette.diff_band)
      .child(
        h_flex()
          .w(width * 2.)
          .flex_none()
          .h_full()
          .gap_1()
          .items_center()
          .justify_center()
          .bg(palette.diff_band_button)
          .when(edge != Edge::Bottom, |buttons| buttons.child(arrow(true)))
          .when(edge != Edge::Top, |buttons| buttons.child(arrow(false))),
      )
      .child(
        div()
          .flex_1()
          .min_w_0()
          .px_3()
          .truncate()
          .font_family(theme.mono_font_family.clone())
          .text_size(FONT_SIZE_SM)
          .text_color(palette.diff_band_text)
          .child(label),
      )
      .into_any_element()
  }
}

/// Which line-number columns a cell shows: both in one column, the old or the new one side by side.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Numbers {
  Both = 0,
  Old = 1,
  New = 2,
}

impl Render for DiffTable {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let size = code_font_size(&self.settings, cx);
    // The setting may have been changed from another diff: the rows are measured again with the new one.
    let wrap = wraps(cx);
    if wrap != self.wrap {
      cx.defer_in(window, move |this, _, cx| {
        this.wrap = wrap;
        this.relayout();
        cx.notify();
      });
    }
    let rows = list(self.list.clone(), cx.processor(|this, ix: usize, _, cx| this.row(ix, cx)));
    let scrolls_sideways = !self.wrap;
    let strip = crate::components::scrollbar_strip(&self.list, "diff-scrollbar", cx);
    h_flex()
      .id("diff-table")
      .key_context("DiffTable")
      .track_focus(&self.focus)
      .on_action(cx.listener(|this, _: &CopySelection, _, cx| this.copy(cx)))
      .size_full()
      .overflow_hidden()
      .bg(palette.background)
      .child(
        div()
          .relative()
          .flex_1()
          .min_w_0()
          .h_full()
          .overflow_hidden()
          .child(
            div()
              .id("diff-horizontal")
              .size_full()
              .when(scrolls_sideways, |body| body.overflow_x_hidden())
              .track_scroll(&self.scroll_x)
              .child(if scrolls_sideways { rows.w(self.content_width(size)).min_w_full().h_full() } else { rows.size_full() }),
          )
          .when(scrolls_sideways, |table| table.child(ScrollableMask::new(Axis::Horizontal, &self.scroll_x).id("diff-horizontal-wheel")))
          .when(scrolls_sideways, |table| table.child(div().absolute().inset_0().child(Scrollbar::horizontal(&self.scroll_x)))),
      )
      .child(strip)
      // A drag over the numbers or the text may end anywhere.
      .on_mouse_up(
        MouseButton::Left,
        cx.listener(|this, _, _, _| {
          this.dragging = false;
          this.selecting_text = false;
        }),
      )
      .on_mouse_up_out(
        MouseButton::Left,
        cx.listener(|this, _, _, _| {
          this.dragging = false;
          this.selecting_text = false;
        }),
      )
  }
}

/// The buttons that move through the changes of a diff table and set how it looks, for the bar above it.
pub(crate) fn table_controls(table: &Entity<DiffTable>) -> impl IntoElement {
  let (previous, next, options) = (table.clone(), table.clone(), table.clone());
  h_flex()
    .gap_1()
    .child(
      Button::new("diff-previous")
        .ghost()
        .xsmall()
        .icon(IconName::ChevronUp)
        .tooltip("Previous change")
        .accessibility_label("Previous change")
        .on_click(move |_, _, cx| previous.update(cx, |table, cx| table.step(false, cx))),
    )
    .child(
      Button::new("diff-next")
        .ghost()
        .xsmall()
        .icon(IconName::ChevronDown)
        .tooltip("Next change")
        .accessibility_label("Next change")
        .on_click(move |_, _, cx| next.update(cx, |table, cx| table.step(true, cx))),
    )
    .child(
      Button::new("diff-options")
        .ghost()
        .xsmall()
        .icon(Icon::empty().path("icons/settings-2.svg"))
        .tooltip("Diff options")
        .accessibility_label("Diff options")
        .dropdown_menu(move |menu, _, cx| {
          let (split, emphasis) = {
            let table = options.read(cx);
            (table.split, table.emphasis)
          };
          let wrap = wraps(cx);
          let mut menu = menu;
          for (label, choice) in [("Unified", false), ("Side by side", true)] {
            let table = options.clone();
            menu = menu.item(
              PopupMenuItem::new(label).checked(split == choice).on_click(move |_, _, cx| table.update(cx, |table, cx| table.set_split(choice, cx))),
            );
          }
          menu = menu.separator().item(PopupMenuItem::new("Wrap long lines").checked(wrap).on_click(move |_, _, cx| {
            set_wraps(!wrap, cx);
            cx.refresh_windows();
          }));
          menu = menu.separator();
          for (label, choice) in [
            ("Highlight changed words", Emphasis::Words),
            ("Highlight changed characters", Emphasis::Characters),
            ("Highlight whole lines", Emphasis::Lines),
          ] {
            let table = options.clone();
            menu = menu.item(
              PopupMenuItem::new(label)
                .checked(emphasis == choice)
                .on_click(move |_, _, cx| table.update(cx, |table, cx| table.set_emphasis(choice, cx))),
            );
          }
          menu
        }),
    )
}
