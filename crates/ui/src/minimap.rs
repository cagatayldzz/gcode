//! An overview of the editor buffer that shares the editor's scroll position.

use gpui_kit::{
  base::input::RopeExt as _,
  component::input::{EditorState, InputEvent},
  *,
};
use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

const MIN_LINES: usize = 80;
const EDIT_DEBOUNCE: Duration = Duration::from_millis(90);
const MIN_VIEWPORT_FRACTION: f32 = 0.025;

mod outline;

use outline::{sample, Line, MAX_COLUMNS};

#[derive(Clone, Copy)]
struct Viewport {
  top: f32,
  height: f32,
}

impl Viewport {
  fn indicator(self) -> Self {
    let height = (self.height * 3.).min(1.);
    let top = (self.top + (self.height - height) / 2.).clamp(0., 1. - height);
    Self { top, height }
  }
}

pub(crate) struct Minimap {
  editor: Entity<EditorState>,
  lines: Arc<[Line]>,
  drag_anchor: Rc<Cell<Option<f32>>>,
  refresh: Option<Task<()>>,
  _changes: Subscription,
}

impl Minimap {
  pub fn new(editor: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
    let changes = cx.subscribe(&editor, |this: &mut Self, _, event: &InputEvent, cx| {
      if matches!(event, InputEvent::Change) {
        this.refresh(true, cx);
      }
    });
    let mut minimap = Self { editor, lines: Arc::default(), drag_anchor: Rc::default(), refresh: None, _changes: changes };
    minimap.refresh(false, cx);
    minimap
  }

  pub fn is_visible(&self, cx: &App) -> bool {
    self.editor.read(cx).text().lines_len() >= MIN_LINES
  }

  fn refresh(&mut self, debounce: bool, cx: &mut Context<Self>) {
    let text = self.editor.read(cx).text();
    let count = text.lines_len();
    if count < MIN_LINES {
      self.refresh = None;
      self.lines = Arc::default();
      return;
    }
    let text = text.clone();
    let background = cx.background_executor().clone();
    self.refresh = Some(cx.spawn(async move |this, cx| {
      if debounce {
        background.timer(EDIT_DEBOUNCE).await;
      }
      let lines = background.spawn(async move { sample(&text) }).await;
      let _ = this.update(cx, |this, cx| {
        this.lines = lines;
        this.refresh = None;
        cx.notify();
      });
    }));
  }
}

fn viewport(editor: &EditorState) -> Option<Viewport> {
  let line_height = f32::from(editor.line_height()?);
  let display = editor.display_map();
  let last_row = display.display_row_count().checked_sub(1)?;
  let first_row = f32::from(-editor.scroll_offset().y).max(0.) / line_height;
  let viewport_rows = f32::from(editor.input_bounds().size.height) / line_height;
  let start = display.display_row_to_buffer_line((first_row.floor() as usize).min(last_row));
  let end = display.display_row_to_buffer_line(((first_row + viewport_rows).ceil() as usize).min(last_row)) + 1;
  let count = editor.text().lines_len();
  let top = start as f32 / count as f32;
  let height = (end.min(count).saturating_sub(start) as f32 / count as f32).max(MIN_VIEWPORT_FRACTION);
  Some(Viewport { top: top.min(1. - height), height })
}

fn fraction(bounds: Bounds<Pixels>, y: Pixels) -> f32 {
  (f32::from(y - bounds.origin.y) / f32::from(bounds.size.height)).clamp(0., 1.)
}

fn scroll(editor: &Entity<EditorState>, position: f32, anchor: f32, cx: &mut App) {
  editor.update(cx, |editor, cx| {
    let Some(line_height) = editor.line_height() else { return };
    let count = editor.text().lines_len();
    let top = (position - anchor).clamp(0., 1.);
    let line = (top * count as f32).round() as usize;
    let row = editor.display_map().buffer_line_to_display_row(line.min(count - 1));
    editor.set_scroll_offset(point(editor.scroll_offset().x, -(line_height * row)), cx);
  });
}

impl Render for Minimap {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = crate::theme::colors(cx);
    let bounds = Rc::new(Cell::new(None::<Bounds<Pixels>>));
    let hit_bounds = bounds.clone();
    let editor = self.editor.clone();
    let drag_anchor = self.drag_anchor.clone();
    let pointer_editor = editor.clone();
    let pointer_anchor = drag_anchor.clone();
    let lines = self.lines.clone();

    div()
      .id("editor-minimap")
      .relative()
      .h_full()
      .w(rems(7.))
      .flex_none()
      .border_l_1()
      .border_color(palette.border)
      .bg(palette.background)
      .cursor_pointer()
      .on_hover(cx.listener(|_, _: &bool, _, cx| cx.notify()))
      .on_mouse_down(MouseButton::Left, move |event, _, cx| {
        let Some(bounds) = hit_bounds.get().filter(|bounds| bounds.size.height > Pixels::ZERO) else { return };
        let Some(viewport) = viewport(pointer_editor.read(cx)) else { return };
        let position = fraction(bounds, event.position.y);
        let indicator = viewport.indicator();
        let inside = (indicator.top..=indicator.top + indicator.height).contains(&position);
        let anchor = if inside { position - viewport.top } else { viewport.height / 2. };
        pointer_anchor.set(Some(anchor));
        if !inside {
          scroll(&pointer_editor, position, anchor, cx);
        }
        cx.stop_propagation();
      })
      .child(
        canvas(
          move |area, _, _| bounds.set(Some(area)),
          move |area, _, window, cx| {
            if area.size.height <= Pixels::ZERO {
              return;
            }
            let inset = window.rem_size() * 0.25;
            let column_width = (area.size.width - inset * 2.) / MAX_COLUMNS as f32;
            let line_step = area.size.height / lines.len().max(1) as f32;
            let stroke_height = line_step.min(window.rem_size() * 0.12) * 0.8;
            for (index, line) in lines.iter().enumerate().filter(|(_, line)| line.start < line.end) {
              let y = area.origin.y + line_step * index;
              let x = area.origin.x + inset + column_width * line.start;
              let width = column_width * (line.end - line.start);
              window.paint_quad(fill(Bounds::new(point(x, y), size(width, stroke_height)), palette.text_secondary.opacity(0.5)));
              let accent_width = column_width * line.accent_end.saturating_sub(line.start);
              if accent_width > Pixels::ZERO {
                window.paint_quad(fill(Bounds::new(point(x, y), size(accent_width, stroke_height)), palette.link.opacity(0.6)));
              }
            }

            let show_viewport = area.contains(&window.mouse_position()) || drag_anchor.get().is_some();
            if let Some(viewport) = show_viewport.then(|| viewport(editor.read(cx))).flatten() {
              let indicator = viewport.indicator();
              let origin = point(area.origin.x + inset, area.origin.y + area.size.height * indicator.top);
              let size = size(area.size.width - inset * 2., area.size.height * indicator.height);
              window.paint_quad(quad(
                Bounds::new(origin, size),
                inset,
                palette.text.opacity(0.13),
                Pixels::ZERO,
                transparent_black(),
                BorderStyle::default(),
              ));
            }

            let anchor = drag_anchor.clone();
            let editor = editor.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
              if phase != DispatchPhase::Capture {
                return;
              }
              if event.pressed_button == Some(MouseButton::Left) {
                if let Some(anchor) = anchor.get() {
                  scroll(&editor, fraction(area, event.position.y), anchor, cx);
                }
              } else {
                anchor.set(None);
              }
            });
            let anchor = drag_anchor.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, _| {
              if phase == DispatchPhase::Capture && event.button == MouseButton::Left && anchor.take().is_some() {
                window.refresh();
              }
            });
          },
        )
        .absolute()
        .size_full(),
      )
  }
}
