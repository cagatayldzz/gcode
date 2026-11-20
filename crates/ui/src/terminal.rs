//! A shell in a view: draws the screen of a [`Session`] and sends it keys, mouse events and pastes.

use crate::{
  selection::{extend_selection, move_endpoint, word_range},
  theme::colors,
  ClearBuffer, CloseTab, CopySelection, NewTerminal, PasteClipboard, SplitDown, SplitRight,
};
use futures::StreamExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    menu::{ContextMenuExt as _, PopupMenuItem},
    scroll::{ScrollableElement as _, ScrollbarHandle},
    ActiveTheme as _,
  },
  *,
};
use std::{
  cell::Cell,
  path::PathBuf,
  rc::Rc,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  time::Duration,
};
use unicode_width::UnicodeWidthChar as _;
use vekro_terminal::{
  dropped_paths, encode_key, encode_mouse, paste_bytes, CursorShape, Modes, Modifiers as TermModifiers, MouseAction, MouseButton as TermButton,
  MouseEvent as TermMouse, Palette, Rgb, Session, SessionEvent, SessionOptions, Snapshot,
};

const FONT_FAMILY: &str = "Geist Mono";
const FONT_SIZE: Pixels = px(12.);
const LINE_HEIGHT: Pixels = px(18.);
const PADDING: Pixels = px(10.);
const LINK_UNDERLINE_OFFSET: Pixels = px(14.);
const LINK_UNDERLINE_THICKNESS: Pixels = px(1.);

/// Sent when the shell of a terminal exits.
pub(crate) struct TerminalExited;

/// Sent when the terminal attention indicator changes.
pub(crate) struct TerminalAttentionChanged;

/// Sent when a foreground job starts or gives the terminal back to the shell.
pub(crate) struct TerminalActivityChanged;

/// A running shell that is not yet in a view.
pub(crate) struct Started {
  session: Session,
  receiver: futures::channel::mpsc::UnboundedReceiver<SessionEvent>,
  pending: Arc<AtomicBool>,
  directory: PathBuf,
  is_shell: bool,
}

pub(crate) struct Terminal {
  session: Rc<Session>,
  title: SharedString,
  attention: bool,
  has_foreground_job: bool,
  directory: Option<PathBuf>,
  focus_handle: FocusHandle,
  content_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
  /// Anchor and end of the selection, as (line, column) cells of the screen.
  selection: Option<((usize, usize), (usize, usize))>,
  selection_dragging: bool,
  /// Fractional lines left over from smooth scrolling.
  scroll_remainder: f32,
  /// The button held while the program receives mouse events, to report its release.
  mouse_button: Option<TermButton>,
  /// The cell of the last mouse event sent to the program, so motion is reported once per cell.
  mouse_cell: Option<(usize, usize)>,
  hovered: bool,
  _events_task: Task<()>,
  _activity_task: Option<Task<()>>,
}

impl EventEmitter<TerminalExited> for Terminal {}

impl EventEmitter<TerminalAttentionChanged> for Terminal {}

impl EventEmitter<TerminalActivityChanged> for Terminal {}

impl Terminal {
  /// Starts `program` (with its arguments) in `directory`, or the login shell for `None`. Kept apart from [`Terminal::new`] so a
  /// failure can be reported before an entity exists.
  pub fn start(directory: PathBuf, program: Option<(String, Vec<String>)>, cx: &App) -> std::io::Result<Started> {
    let (sender, receiver) = futures::channel::mpsc::unbounded::<SessionEvent>();
    let pending = Arc::new(AtomicBool::new(false));
    let notify_pending = pending.clone();
    let is_shell = program.is_none();
    let options = SessionOptions { program, working_directory: Some(directory.clone()), palette: Some(palette(cx)) };
    let session = Session::spawn(options, 80, 24, move |event| {
      // Redraws are coalesced: one queued wakeup covers any number that follow.
      if event == SessionEvent::Wakeup && notify_pending.swap(true, Ordering::AcqRel) {
        return;
      }
      let _ = sender.unbounded_send(event);
    })?;
    Ok(Started { session, receiver, pending, directory, is_shell })
  }

  pub fn new(started: Started, cx: &mut Context<Self>) -> Self {
    let Started { session, mut receiver, pending, directory, is_shell } = started;
    let title = std::env::var("SHELL").ok().and_then(|path| path.rsplit('/').next().map(|name| name.to_string())).unwrap_or_else(|| "shell".into());
    let events_task = cx.spawn(async move |this, cx| {
      while let Some(event) = receiver.next().await {
        pending.store(false, Ordering::Release);
        if this.update(cx, |this, cx| this.handle_event(event, cx)).is_err() {
          break;
        }
      }
    });
    let activity_task = is_shell.then(|| session.foreground_job_probe()).flatten().map(|probe| {
      cx.spawn(async move |this, cx| {
        let executor = cx.background_executor().clone();
        let mut previous = false;
        loop {
          executor.timer(Duration::from_millis(250)).await;
          let probe = probe.clone();
          let active = executor.spawn(async move { probe() }).await;
          if active != previous {
            previous = active;
            if this
              .update(cx, |terminal, cx| {
                terminal.has_foreground_job = active;
                cx.emit(TerminalActivityChanged);
              })
              .is_err()
            {
              break;
            }
          }
        }
      })
    });
    Self {
      session: Rc::new(session),
      title: title.into(),
      attention: false,
      has_foreground_job: false,
      directory: Some(directory),
      focus_handle: cx.focus_handle(),
      content_bounds: Rc::new(Cell::new(None)),
      selection: None,
      selection_dragging: false,
      scroll_remainder: 0.,
      mouse_button: None,
      mouse_cell: None,
      hovered: false,
      _events_task: events_task,
      _activity_task: activity_task,
    }
  }

  /// Types `text` into the program as if it was pasted.
  pub fn insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
    self.clear_attention(cx);
    self.session.write(paste_bytes(text, self.session.modes().bracketed_paste));
    self.selection = None;
    cx.notify();
  }

  /// What the shell calls itself (the window title it reports), or the shell's name.
  pub fn title(&self) -> SharedString {
    self.title.clone()
  }

  pub fn needs_attention(&self) -> bool {
    self.attention
  }

  pub fn has_foreground_job(&self) -> bool {
    self.has_foreground_job
  }

  fn clear_attention(&mut self, cx: &mut Context<Self>) {
    if self.attention {
      self.attention = false;
      cx.emit(TerminalAttentionChanged);
    }
  }

  fn handle_event(&mut self, event: SessionEvent, cx: &mut Context<Self>) {
    match event {
      SessionEvent::Wakeup => {}
      SessionEvent::Bell => {
        self.attention = true;
        cx.emit(TerminalAttentionChanged);
      }
      SessionEvent::Title(title) => self.title = title.into(),
      SessionEvent::Directory(path) => self.directory = Some(path),
      SessionEvent::ClipboardStore(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
      SessionEvent::Exit => cx.emit(TerminalExited),
    }
    cx.notify();
  }

  fn copy(&mut self, cx: &mut Context<Self>) {
    let Some((anchor, end)) = self.selection.filter(|(anchor, end)| anchor != end) else { return };
    let (start, end) = if anchor < end { (anchor, end) } else { (end, anchor) };
    let snapshot = self.session.snapshot(&palette(cx));
    let last = end.0.min(snapshot.lines.len().saturating_sub(1));
    let mut copied = String::new();
    for line in start.0..=last {
      let text: String = snapshot.lines[line].iter().map(|span| span.text.as_str()).collect();
      let from = if line == start.0 { start.1 } else { 0 };
      let to = if line == end.0 { end.1 } else { snapshot.columns };
      let mut column = 0;
      let selected: String = text
        .chars()
        .filter(|character| {
          let width = character.width().unwrap_or(0);
          let include = column < to && column + width > from;
          column += width;
          include
        })
        .collect();
      // A line that only wrapped at the screen edge is one line of text: keep it whole.
      if line < last && snapshot.wrapped.get(line).copied().unwrap_or(false) {
        copied.push_str(&selected);
      } else {
        copied.push_str(selected.trim_end());
        copied.push('\n');
      }
    }
    cx.write_to_clipboard(ClipboardItem::new_string(copied.trim_end_matches('\n').to_owned()));
  }

  /// Clears the history above the screen.
  fn clear(&mut self, cx: &mut Context<Self>) {
    self.session.clear_history();
    self.selection = None;
    cx.notify();
  }

  fn paste(&mut self, cx: &mut Context<Self>) {
    let Some(item) = cx.read_from_clipboard() else { return };
    self.clear_attention(cx);
    if item.entries.iter().any(|entry| matches!(entry, ClipboardEntry::Image(_))) {
      // Programs such as Codex read image data from the clipboard themselves.
      self.session.write(vec![0x16]);
    } else if let Some(text) = item.text() {
      self.session.write(paste_bytes(&text, self.session.modes().bracketed_paste));
    }
    self.selection = None;
    cx.notify();
  }

  fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
    let keystroke = &event.keystroke;
    let held = keystroke.modifiers;
    if held.platform && !held.control && !held.alt {
      match keystroke.key.as_str() {
        "c" => {
          self.copy(cx);
          cx.stop_propagation();
          return;
        }
        "v" => {
          self.paste(cx);
          cx.stop_propagation();
          return;
        }
        "k" => {
          self.clear(cx);
          cx.stop_propagation();
          return;
        }
        _ => {}
      }
    }
    // ⇧PageUp, ⇧PageDown, ⇧Home and ⇧End move through the history instead of reaching the program.
    let paging =
      held.shift && !held.control && !held.alt && !held.platform && matches!(keystroke.key.as_str(), "pageup" | "pagedown" | "home" | "end");
    if paging && !self.session.modes().alt_screen {
      match keystroke.key.as_str() {
        "pageup" => self.session.scroll_page(true),
        "pagedown" => self.session.scroll_page(false),
        "home" => self.session.scroll_to(usize::MAX),
        _ => self.session.scroll_to(0),
      };
      self.selection = None;
      cx.stop_propagation();
      cx.notify();
      return;
    }
    // ⇧ with an arrow key moves the end of the selection.
    if held.shift && !held.control && !held.alt && !held.platform && matches!(keystroke.key.as_str(), "up" | "down" | "left" | "right") {
      if let Some((anchor, end)) = self.selection {
        let snapshot = self.session.snapshot(&palette(cx));
        let lines: Vec<_> = (0..snapshot.lines.len()).map(|line| snapshot.line_text(line)).collect();
        self.selection = Some((anchor, move_endpoint(&lines, end, &keystroke.key)));
        cx.stop_propagation();
        cx.notify();
        return;
      }
    }
    let modifiers = TermModifiers { control: held.control, alt: held.alt, shift: held.shift, command: held.platform };
    let Some(bytes) = encode_key(&keystroke.key, keystroke.key_char.as_deref(), modifiers, self.session.modes().app_cursor) else { return };
    self.clear_attention(cx);
    self.session.write(bytes);
    self.selection = None;
    cx.stop_propagation();
    cx.notify();
  }
}

impl Focusable for Terminal {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus_handle.clone()
  }
}

/// The terminal colors for the current light or dark theme.
pub(crate) fn palette(cx: &App) -> Palette {
  let dark = cx.theme().is_dark();
  let mut palette = if dark { Palette::dark() } else { Palette::light() };
  let theme = colors(cx);
  let terminal_color = |color: Hsla| {
    let color: Rgba = color.into();
    Rgb::new((color.r * 255.).round() as u8, (color.g * 255.).round() as u8, (color.b * 255.).round() as u8)
  };
  palette.background = terminal_color(theme.background);
  palette.foreground = terminal_color(theme.text);
  palette
}

/// Lets the Kit scrollbar drive the scrollback: the offset is how far the view sits above the live screen.
#[derive(Clone)]
struct TerminalScroll {
  session: Rc<Session>,
  bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
  history: usize,
  scrolled: usize,
}

impl ScrollbarHandle for TerminalScroll {
  fn viewport_bounds(&self) -> Bounds<Pixels> {
    self.bounds.get().unwrap_or_default()
  }

  fn offset(&self) -> Point<Pixels> {
    point(px(0.), -(LINE_HEIGHT * self.history.saturating_sub(self.scrolled) as f32))
  }

  fn set_offset(&self, offset: Point<Pixels>) {
    let lines_above = (f32::from(-offset.y) / f32::from(LINE_HEIGHT)).round().max(0.) as usize;
    self.session.scroll_to(self.history.saturating_sub(lines_above));
  }

  fn content_size(&self) -> Size<Pixels> {
    let viewport = self.viewport_bounds().size;
    size(viewport.width, viewport.height + LINE_HEIGHT * self.history as f32)
  }
}

fn color(rgb: Rgb) -> Hsla {
  gpui_kit::rgb(rgb.hex()).into()
}

fn cell_at(bounds: Bounds<Pixels>, position: Point<Pixels>, cell_width: f32) -> Option<(usize, usize)> {
  let x = f32::from(position.x - bounds.origin.x - PADDING);
  let y = f32::from(position.y - bounds.origin.y - PADDING);
  (bounds.contains(&position) && x >= 0. && y >= 0.).then(|| ((y / f32::from(LINE_HEIGHT)).floor() as usize, (x / cell_width).floor() as usize))
}

fn is_web_link(target: &str) -> bool {
  target.starts_with("http://") || target.starts_with("https://")
}

/// The cell under the mouse, held inside the screen, for programs that follow the mouse.
fn clamped_cell(bounds: Bounds<Pixels>, position: Point<Pixels>, cell_width: f32, columns: usize, lines: usize) -> (usize, usize) {
  let x = f32::from(position.x - bounds.origin.x - PADDING);
  let y = f32::from(position.y - bounds.origin.y - PADDING);
  let line = ((y / f32::from(LINE_HEIGHT)).floor().max(0.) as usize).min(lines.saturating_sub(1));
  let column = ((x / cell_width).floor().max(0.) as usize).min(columns.saturating_sub(1));
  (line, column)
}

fn report_mouse(session: &Session, modes: Modes, button: TermButton, action: MouseAction, (line, column): (usize, usize), held: &Modifiers) {
  let modifiers = TermModifiers { control: held.control, alt: held.alt, shift: held.shift, command: false };
  if let Some(bytes) = encode_mouse(TermMouse { button, action, column, line, modifiers }, modes.sgr_mouse) {
    session.report(bytes);
  }
}

impl Terminal {
  fn release_mouse(&mut self, position: Point<Pixels>, held: &Modifiers, cell_width: f32, size: (usize, usize)) {
    self.selection_dragging = false;
    let (Some(button), Some(bounds)) = (self.mouse_button.take(), self.content_bounds.get()) else { return };
    self.mouse_cell = None;
    let cell = clamped_cell(bounds, position, cell_width, size.0, size.1);
    report_mouse(&self.session, self.session.modes(), button, MouseAction::Release, cell, held);
  }
}

impl Render for Terminal {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = palette(cx);
    self.session.set_palette(palette.clone());
    let snapshot = Rc::new(self.session.snapshot(&palette));
    let font = font(FONT_FAMILY);
    let text_system = window.text_system();
    let font_id = text_system.resolve_font(&font);
    let cell_width_px = text_system.advance(font_id, FONT_SIZE, 'm').map(|size| size.width).unwrap_or(FONT_SIZE * 0.6);
    let cell_width = f32::from(cell_width_px);
    let session = self.session.clone();
    let content_bounds = self.content_bounds.clone();
    let columns = snapshot.columns;
    let line_count = snapshot.lines.len();
    let selection = self.selection;
    let selection_color = cx.theme().selection;
    let reporting = self.session.modes().mouse_reporting();
    let focused = self.focus_handle.is_focused(window);
    let scroll_handle = TerminalScroll {
      session: self.session.clone(),
      bounds: self.content_bounds.clone(),
      history: snapshot.history_size,
      scrolled: snapshot.scrolled,
    };
    let has_scrollbar = snapshot.history_size > 0;
    let link_cell = (self.hovered && window.is_window_active() && window.modifiers().platform)
      .then(|| content_bounds.get().and_then(|bounds| cell_at(bounds, window.mouse_position(), cell_width)))
      .flatten();
    let hovered_link = link_cell.and_then(|(line, column)| {
      let (target, start, end) = snapshot.link_range_at(line, column)?;
      is_web_link(&target).then_some((line, start, end))
    });
    let clicked_snapshot = snapshot.clone();

    div()
      .id("terminal")
      .key_context("Terminal")
      .track_focus(&self.focus_handle)
      .on_key_down(cx.listener(Self::on_key_down))
      .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
        let text = dropped_paths(paths.paths().iter().map(|path| path.as_path()));
        this.session.write(paste_bytes(&text, this.session.modes().bracketed_paste));
        this.focus_handle.focus(window, cx);
        this.selection = None;
        cx.notify();
      }))
      .on_action(cx.listener(|this, _: &CopySelection, _, cx| this.copy(cx)))
      .on_action(cx.listener(|this, _: &PasteClipboard, _, cx| this.paste(cx)))
      .on_action(cx.listener(|this, _: &ClearBuffer, _, cx| this.clear(cx)))
      // A right click puts the keyboard in this shell, so the menu's Copy, Paste and Clear act on it.
      .on_mouse_down(MouseButton::Right, cx.listener(|this, _: &MouseDownEvent, window, cx| this.focus_handle.focus(window, cx)))
      .relative()
      .size_full()
      .overflow_hidden()
      .bg(color(palette.background))
      .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
      .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
        this.hovered = *hovered;
        cx.notify();
      }))
      .on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
          this.focus_handle.focus(window, cx);
          this.selection_dragging = false;
          let modes = this.session.modes();
          let Some(bounds) = this.content_bounds.get() else { return };
          if modes.mouse_reporting() && !event.modifiers.shift && !event.modifiers.platform {
            // The program follows the mouse itself; holding Shift selects text instead.
            let cell = clamped_cell(bounds, event.position, cell_width, columns, line_count);
            this.selection = None;
            this.mouse_button = Some(TermButton::Left);
            this.mouse_cell = Some(cell);
            report_mouse(&this.session, modes, TermButton::Left, MouseAction::Press, cell, &event.modifiers);
            return;
          }
          let Some((line, column)) = cell_at(bounds, event.position, cell_width) else { return };
          if event.modifiers.platform {
            if let Some(target) = clicked_snapshot.link_at(line, column).filter(|target| is_web_link(target)) {
              cx.open_url(&target);
              return;
            }
          }
          let position = (line.min(line_count.saturating_sub(1)), column.min(columns));
          let text = this.session.snapshot(&palette_for(cx)).line_text(position.0);
          let extend = event.modifiers.shift && event.click_count < 2;
          this.selection = if let (true, Some(selection)) = (extend, this.selection) {
            Some(extend_selection(&text, selection, position))
          } else if event.click_count < 2 {
            Some((position, position))
          } else if event.click_count >= 3 {
            let width = text.chars().map(|character| character.width().unwrap_or(0)).sum();
            Some(((position.0, 0), (position.0, width)))
          } else {
            word_range(&text, position.1).map(|(start, end)| ((position.0, start), (position.0, end)))
          };
          this.selection_dragging = this.selection.is_some();
          cx.notify();
        }),
      )
      .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
        let Some(bounds) = this.content_bounds.get() else { return };
        let cell = if event.modifiers.platform { cell_at(bounds, event.position, cell_width) } else { None };
        if cell != link_cell {
          cx.notify();
        }
        if event.pressed_button == Some(MouseButton::Left) && !cx.has_active_drag() {
          if let Some((anchor, _)) = this.selection.filter(|_| this.selection_dragging) {
            let x = f32::from(event.position.x - bounds.origin.x - PADDING);
            let y = f32::from(event.position.y - bounds.origin.y - PADDING);
            let line = (y / f32::from(LINE_HEIGHT)).floor().max(0.) as usize;
            let column = (x / cell_width).round().max(0.) as usize;
            this.selection = Some((anchor, (line.min(line_count.saturating_sub(1)), column.min(columns))));
            cx.notify();
          }
        } else {
          this.selection_dragging = false;
        }
        if reporting && !event.modifiers.shift && (!event.modifiers.platform || this.mouse_button.is_some()) {
          let modes = this.session.modes();
          let wanted = if this.mouse_button.is_some() { modes.mouse_drag || modes.mouse_motion } else { modes.mouse_motion };
          let cell = clamped_cell(bounds, event.position, cell_width, columns, line_count);
          if wanted && this.mouse_cell != Some(cell) {
            this.mouse_cell = Some(cell);
            let (button, action) = match this.mouse_button {
              Some(button) => (button, MouseAction::Drag),
              None => (TermButton::None, MouseAction::Move),
            };
            report_mouse(&this.session, modes, button, action, cell, &event.modifiers);
          }
        }
      }))
      .on_mouse_up(
        MouseButton::Left,
        cx.listener(move |this, event: &MouseUpEvent, _, _| this.release_mouse(event.position, &event.modifiers, cell_width, (columns, line_count))),
      )
      .on_mouse_up_out(
        MouseButton::Left,
        cx.listener(move |this, event: &MouseUpEvent, _, _| this.release_mouse(event.position, &event.modifiers, cell_width, (columns, line_count))),
      )
      .on_mouse_down(
        MouseButton::Middle,
        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
          this.focus_handle.focus(window, cx);
          if !this.session.modes().mouse_reporting() {
            this.paste(cx);
          }
        }),
      )
      .on_scroll(cx.listener(move |this, event: &ScrollEvent, _, cx| {
        this.scroll_remainder += f32::from(event.delta.pixel_delta(LINE_HEIGHT).y) / f32::from(LINE_HEIGHT);
        let lines = this.scroll_remainder.trunc();
        if lines == 0. {
          return;
        }
        this.scroll_remainder -= lines;
        let modes = this.session.modes();
        let (up, count) = (lines > 0., (lines.abs() as usize).min(20));
        if modes.mouse_reporting() && !event.modifiers.shift {
          // A program that follows the mouse scrolls itself.
          if let Some(bounds) = this.content_bounds.get() {
            let cell = clamped_cell(bounds, event.position, cell_width, columns, line_count);
            let button = if up { TermButton::WheelUp } else { TermButton::WheelDown };
            for _ in 0..count {
              report_mouse(&this.session, modes, button, MouseAction::Press, cell, &event.modifiers);
            }
          }
        } else if modes.alt_screen && modes.alternate_scroll {
          // Pagers and editors have no history; the wheel moves their cursor like the arrow keys.
          if let Some(bytes) = encode_key(if up { "up" } else { "down" }, None, Default::default(), modes.app_cursor) {
            for _ in 0..count {
              this.session.report(bytes.clone());
            }
          }
        } else if this.session.scroll(lines as i32) {
          // The selection is tied to screen positions, which just moved.
          this.selection = None;
          cx.notify();
        }
      }))
      // Fits the PTY to the space this panel was given.
      .child(
        canvas(
          move |bounds, window, _| {
            content_bounds.set(Some(bounds));
            let columns = ((f32::from(bounds.size.width) - 2. * f32::from(PADDING)) / cell_width).floor().max(2.) as usize;
            let lines = ((f32::from(bounds.size.height) - 2. * f32::from(PADDING)) / f32::from(LINE_HEIGHT)).floor().max(1.) as usize;
            if session.resize(columns, lines) {
              window.refresh();
            }
          },
          |_, _, _, _| {},
        )
        .absolute()
        .size_full(),
      )
      .when(!reporting && hovered_link.is_none(), |this| this.cursor_text())
      .child(
        // Out of the flow, so the panel's size comes only from the space layout gives it.
        div()
          .absolute()
          .size_full()
          .overflow_hidden()
          .p(PADDING)
          .text_size(FONT_SIZE)
          .font_family(font.family.clone())
          .text_color(color(palette.foreground))
          .child(rows(&snapshot, &font)),
      )
      .when_some(selection, |this, (anchor, end)| {
        let (start, end) = if anchor < end { (anchor, end) } else { (end, anchor) };
        this.children((start.0..=end.0).filter_map(|line| {
          let text_width: usize = snapshot.line_text(line).chars().map(|character| character.width().unwrap_or(0)).sum();
          let from = if line == start.0 { start.1 } else { 0 };
          let to = (if line == end.0 { end.1 } else { columns }).min(text_width);
          (to > from).then(|| {
            div()
              .absolute()
              .left(PADDING + cell_width_px * from as f32)
              .top(PADDING + LINE_HEIGHT * line as f32)
              .w(cell_width_px * (to - from) as f32)
              .h(LINE_HEIGHT)
              .bg(selection_color.opacity(0.4))
          })
        }))
      })
      .when_some(hovered_link, |this, (line, start, end)| {
        this.cursor_pointer().child(
          div()
            .absolute()
            .left(PADDING + cell_width_px * start as f32)
            .top(PADDING + LINE_HEIGHT * line as f32 + LINK_UNDERLINE_OFFSET)
            .w(cell_width_px * (end - start) as f32)
            .h(LINK_UNDERLINE_THICKNESS)
            .bg(color(palette.foreground)),
        )
      })
      .when_some(snapshot.cursor.filter(|cursor| focused && cursor.shape != CursorShape::Block), |this, cursor| {
        let (width, height, offset) = match cursor.shape {
          CursorShape::Beam => (px(2.), LINE_HEIGHT, px(0.)),
          _ => (cell_width_px, px(2.), LINE_HEIGHT - px(2.)),
        };
        this.child(
          div()
            .absolute()
            .left(PADDING + cell_width_px * cursor.column as f32)
            .top(PADDING + LINE_HEIGHT * cursor.line as f32 + offset)
            .w(width)
            .h(height)
            .bg(color(palette.foreground)),
        )
      })
      .when(has_scrollbar, |this| this.child(div().id("terminal-scrollbar").absolute().size_full().vertical_scrollbar(&scroll_handle)))
      .context_menu(|menu, _, _| {
        menu
          .item(PopupMenuItem::new("New Tab").action(Box::new(NewTerminal)))
          .separator()
          .item(PopupMenuItem::new("Split Pane Vertically").action(Box::new(SplitRight)))
          .item(PopupMenuItem::new("Split Pane Horizontally").action(Box::new(SplitDown)))
          .separator()
          .item(PopupMenuItem::new("Copy").action(Box::new(CopySelection)))
          .item(PopupMenuItem::new("Paste").action(Box::new(PasteClipboard)))
          .item(PopupMenuItem::new("Clear Buffer").action(Box::new(ClearBuffer)))
          .separator()
          .item(PopupMenuItem::new("Close").action(Box::new(CloseTab)))
      })
  }
}

fn palette_for(cx: &App) -> Palette {
  palette(cx)
}

fn rows(snapshot: &Snapshot, base: &Font) -> impl IntoElement {
  let bold = Font { weight: FontWeight::BOLD, ..base.clone() };
  let italic = Font { style: FontStyle::Italic, ..base.clone() };
  let bold_italic = Font { weight: FontWeight::BOLD, style: FontStyle::Italic, ..base.clone() };
  let lines = snapshot.lines.iter().enumerate().map(|(index, spans)| {
    let mut text = String::new();
    let mut runs = Vec::with_capacity(spans.len());
    for span in spans {
      text.push_str(&span.text);
      let foreground = color(span.foreground);
      runs.push(TextRun {
        len: span.text.len(),
        font: match (span.bold, span.italic) {
          (true, true) => bold_italic.clone(),
          (true, false) => bold.clone(),
          (false, true) => italic.clone(),
          (false, false) => base.clone(),
        },
        color: foreground,
        background_color: span.background.map(color),
        underline: span.underline.then(|| UnderlineStyle { thickness: px(1.), color: Some(foreground), wavy: false }),
        strikethrough: span.strikeout.then(|| StrikethroughStyle { thickness: px(1.), color: Some(foreground) }),
      });
    }
    div()
      .id(("line", index))
      .h(LINE_HEIGHT)
      .w_full()
      .flex_shrink_0()
      .overflow_hidden()
      .whitespace_nowrap()
      .child(StyledText::new(text).with_runs(runs))
  });
  div().w_full().children(lines)
}
