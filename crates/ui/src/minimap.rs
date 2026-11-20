//! An overview of the editor buffer that shares the editor's scroll position.

use gpui_kit::{
  component::input::{EditorState, InputEvent},
  component::{ActiveTheme as _, Theme},
  *,
};
use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

const EDIT_DEBOUNCE: Duration = Duration::from_millis(90);
const INDICATOR_HEIGHT_REMS: f32 = 4.;
const MAX_LINE_STEP_REMS: f32 = 0.16;
const MAX_FONT_SIZE_REMS: f32 = 0.125;

mod outline;

pub(crate) use outline::{dotenv_styles, gitignore_styles};
use outline::{sample, Line};

pub(crate) struct Minimap {
  editor: Entity<EditorState>,
  lines: Arc<[Line]>,
  language: &'static str,
  drag_anchor: Rc<Cell<Option<Pixels>>>,
  refresh: Option<Task<()>>,
  _changes: Subscription,
  _theme: Subscription,
}

impl Minimap {
  pub fn new(editor: Entity<EditorState>, language: &'static str, cx: &mut Context<Self>) -> Self {
    let changes = cx.subscribe(&editor, |this: &mut Self, _, event: &InputEvent, cx| {
      if matches!(event, InputEvent::Change) {
        this.refresh(true, cx);
      }
    });
    let theme = cx.observe_global::<Theme>(|this, cx| this.refresh(true, cx));
    let mut minimap = Self { editor, language, lines: Arc::default(), drag_anchor: Rc::default(), refresh: None, _changes: changes, _theme: theme };
    minimap.refresh(false, cx);
    minimap
  }

  fn refresh(&mut self, debounce: bool, cx: &mut Context<Self>) {
    let text = self.editor.read(cx).text().clone();
    let language = self.language;
    let theme = cx.theme().highlight_theme.clone();
    let background = cx.background_executor().clone();
    self.refresh = Some(cx.spawn(async move |this, cx| {
      if debounce {
        background.timer(EDIT_DEBOUNCE).await;
      }
      let lines = background.spawn(async move { sample(&text, language, &theme) }).await;
      let _ = this.update(cx, |this, cx| {
        this.lines = lines;
        this.refresh = None;
        cx.notify();
      });
    }));
  }
}

fn scroll_range(editor: &EditorState) -> Option<Pixels> {
  let line_height = editor.line_height()?;
  let viewport = editor.input_bounds().size.height;
  // GPUI Kit's code editor reserves half a viewport below the last line, with a three-row minimum.
  let bottom_padding = (viewport / 2.).max(line_height * 3.);
  let content = line_height * editor.display_map().wrap_row_count() + bottom_padding;
  Some((content - viewport).max(Pixels::ZERO))
}

fn scroll_progress(editor: &EditorState, range: Pixels) -> f32 {
  (f32::from(-editor.scroll_offset().y) / f32::from(range)).clamp(0., 1.)
}

fn indicator_height(track: Bounds<Pixels>, window: &Window) -> Pixels {
  (window.rem_size() * INDICATOR_HEIGHT_REMS).min(track.size.height)
}

fn line_step(track: Bounds<Pixels>, lines: usize, window: &Window) -> Pixels {
  (track.size.height / lines.max(1) as f32).min(window.rem_size() * MAX_LINE_STEP_REMS)
}

fn indicator_travel(track: Bounds<Pixels>, height: Pixels, lines: usize, window: &Window) -> Pixels {
  (track.size.height - height).min(line_step(track, lines, window) * lines)
}

fn scroll(editor: &Entity<EditorState>, position: Pixels, travel: Pixels, cx: &mut App) {
  if travel <= Pixels::ZERO {
    return;
  }
  editor.update(cx, |editor, cx| {
    let Some(range) = scroll_range(editor) else { return };
    let progress = (f32::from(position) / f32::from(travel)).clamp(0., 1.);
    editor.set_scroll_offset(point(editor.scroll_offset().x, -(range * progress)), cx);
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
    let line_count = lines.len();

    div()
      .id("editor-minimap")
      .relative()
      .h_full()
      .w(rems(7.))
      .flex_none()
      .border_l_1()
      .border_color(palette.border)
      .bg(palette.background)
      .cursor_default()
      .on_hover(cx.listener(|_, _: &bool, _, cx| cx.notify()))
      .on_mouse_down(MouseButton::Left, move |event, window, cx| {
        let Some(bounds) = hit_bounds.get().filter(|bounds| bounds.size.height > Pixels::ZERO) else { return };
        let editor = pointer_editor.read(cx);
        let Some(range) = scroll_range(editor).filter(|range| *range > Pixels::ZERO) else { return };
        let height = indicator_height(bounds, window);
        let travel = indicator_travel(bounds, height, line_count, window);
        let top = travel * scroll_progress(editor, range);
        let position = event.position.y - bounds.origin.y;
        let inside = (top..=top + height).contains(&position);
        let anchor = if inside { position - top } else { height / 2. };
        pointer_anchor.set(Some(anchor));
        if !inside {
          scroll(&pointer_editor, position - anchor, travel, cx);
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
            let line_step = line_step(area, line_count, window);
            let font_size = line_step.min(window.rem_size() * MAX_FONT_SIZE_REMS);
            let mono_font = font(cx.theme().mono_font_family.clone());
            let clip = Bounds::new(point(area.origin.x + inset, area.origin.y), size(area.size.width - inset * 2., area.size.height));
            window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
              let mut runs = Vec::new();
              for (index, line) in lines.iter().enumerate().filter(|(_, line)| !line.text.is_empty()) {
                runs.clear();
                runs.extend(line.colors.iter().map(|(len, color)| TextRun {
                  len: *len,
                  font: mono_font.clone(),
                  color: color.unwrap_or(palette.text_secondary),
                  background_color: None,
                  underline: None,
                  strikethrough: None,
                }));
                let shaped = window.text_system().shape_line(line.text.clone(), font_size, &runs, None);
                let origin = point(clip.origin.x, clip.origin.y + line_step * index);
                let _ = shaped.paint(origin, line_step, TextAlign::Left, None, window, cx);
              }
            });

            let height = indicator_height(area, window);
            let travel = indicator_travel(area, height, line_count, window);
            let show_indicator = area.contains(&window.mouse_position()) || drag_anchor.get().is_some();
            let state = editor.read(cx);
            if let Some(range) = show_indicator.then(|| scroll_range(state)).flatten().filter(|range| *range > Pixels::ZERO) {
              let top = travel * scroll_progress(state, range);
              let origin = point(area.origin.x + inset, area.origin.y + top);
              let size = size(area.size.width - inset * 2., height);
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
                  scroll(&editor, event.position.y - area.origin.y - anchor, travel, cx);
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
