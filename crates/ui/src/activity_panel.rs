//! The activity log panel at the bottom of the window, opened by the debug button of the title bar: what the app did (Git commands,
//! requests to services, steps of the interface) with how long each took and how it ended, oldest first, kept up to date while it is
//! open. Reads `ws_activity`.
//!
//! The lines are shown in the Kit's read-only `Editor`, so they can be selected with the mouse, scrolled and copied like any text.
//! While text is selected the list is not replaced (that would drop the selection); a list the user scrolled up in keeps its place;
//! a list at the bottom follows the new lines.

use crate::theme::{colors, FONT_SIZE_SM};
use gpui_kit::{base::Selectable as _, prelude::FluentBuilder as _};
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Editor, EditorState},
    label::Label,
    notification::Notification,
    v_flex, Icon, IconName, Sizable as _, WindowExt as _,
  },
  *,
};
use std::time::Duration;
use ws_activity::{Entry, Level};

/// How often the window looks for new lines.
const POLL: Duration = Duration::from_millis(300);

/// How close to the end (in pixels) the list counts as being at the bottom.
const BOTTOM_SLACK: f64 = 8.;

/// Sent when the panel's close button is pressed.
pub(crate) struct ActivityClosed;

impl EventEmitter<ActivityClosed> for ActivityLog {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
  All,
  Slow,
  Errors,
}

impl Filter {
  fn label(self) -> &'static str {
    match self {
      Self::All => "All",
      Self::Slow => "Slow & errors",
      Self::Errors => "Errors",
    }
  }

  fn matches(self, entry: &Entry) -> bool {
    match self {
      Self::All => true,
      Self::Slow => entry.level != Level::Info,
      Self::Errors => entry.level == Level::Error,
    }
  }
}

pub(crate) struct ActivityLog {
  entries: Vec<Entry>,
  seen: u64,
  filter: Filter,
  editor: Entity<EditorState>,
  /// Lines came in while text was selected, and are shown when the selection is gone.
  waiting: bool,
  _poll: Task<()>,
}

impl ActivityLog {
  pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let entries = ws_activity::entries();
    let text = Self::text_of(&entries, Filter::All);
    let editor = cx.new(|cx| EditorState::new(window, cx).default_value(text));
    let poll = cx.spawn_in(window, async move |this, cx| loop {
      cx.background_executor().timer(POLL).await;
      if this.update_in(cx, |this, window, cx| this.refresh(false, window, cx)).is_err() {
        break;
      }
    });
    let mut log = Self { entries, seen: ws_activity::version(), filter: Filter::All, editor, waiting: false, _poll: poll };
    log.scroll_to_end(cx);
    log
  }

  fn text_of(entries: &[Entry], filter: Filter) -> String {
    entries.iter().filter(|entry| filter.matches(entry)).map(Entry::text).collect::<Vec<_>>().join("\n")
  }

  fn scroll_to_end(&mut self, cx: &mut Context<Self>) {
    // The editor clamps the offset to the end of the text once it has laid the new text out.
    self.editor.update(cx, |editor, cx| editor.set_scroll_y_f64(-1.0e9, cx));
  }

  /// Takes the new lines, when there are any (or `force`, when the filter changed).
  fn refresh(&mut self, force: bool, window: &mut Window, cx: &mut Context<Self>) {
    let version = ws_activity::version();
    if version == self.seen && !force && !self.waiting {
      return;
    }
    let (selected, scrolled, at_bottom) = {
      let editor = self.editor.read(cx);
      let viewport = f64::from(f32::from(editor.input_bounds().size.height));
      let at_bottom = editor.content_height_f64() + editor.scroll_y_f64() <= viewport + BOTTOM_SLACK;
      (!editor.selected_text().to_string().is_empty(), editor.scroll_y_f64(), at_bottom)
    };
    // Replacing the text would drop the selection: wait until it is gone.
    if selected && !force {
      if !self.waiting {
        self.waiting = true;
        cx.notify();
      }
      return;
    }
    self.waiting = false;
    self.seen = version;
    self.entries = ws_activity::entries();
    let text = Self::text_of(&self.entries, self.filter);
    self.editor.update(cx, |editor, cx| editor.set_value(text, window, cx));
    if force || at_bottom {
      self.scroll_to_end(cx);
    } else {
      // Lines are added at the end, so the place the user scrolled to is the same pixel.
      self.editor.update(cx, |editor, cx| editor.set_scroll_y_f64(scrolled, cx));
    }
    cx.notify();
  }

  fn set_filter(&mut self, filter: Filter, window: &mut Window, cx: &mut Context<Self>) {
    self.filter = filter;
    self.refresh(true, window, cx);
  }

  fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    ws_activity::clear();
    self.refresh(true, window, cx);
  }

  /// Copies the selected text, or every line the filter lets through when nothing is selected.
  fn copy(&self, window: &mut Window, cx: &mut Context<Self>) {
    let selected = self.editor.read(cx).selected_text().to_string();
    let (text, what) = if selected.is_empty() {
      let count = self.entries.iter().filter(|entry| self.filter.matches(entry)).count();
      (Self::text_of(&self.entries, self.filter), format!("{count} log {}", if count == 1 { "line" } else { "lines" }))
    } else {
      let count = selected.lines().count();
      (selected, format!("{count} selected {}", if count == 1 { "line" } else { "lines" }))
    };
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    window.push_notification(Notification::info(format!("Copied {what}.")), cx);
  }
}

impl Render for ActivityLog {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let (errors, slow) = self.entries.iter().fold((0, 0), |(errors, slow), entry| match entry.level {
      Level::Error => (errors + 1, slow),
      Level::Slow => (errors, slow + 1),
      Level::Info => (errors, slow),
    });
    let chip = |filter: Filter, cx: &mut Context<Self>| {
      Button::new(filter.label())
        .small()
        .ghost()
        .label(filter.label())
        .selected(self.filter == filter)
        .on_click(cx.listener(move |this, _, window, cx| this.set_filter(filter, window, cx)))
    };
    v_flex()
      .id("activity-panel")
      .size_full()
      .border_t_1()
      .border_color(palette.border)
      .bg(palette.background)
      .text_color(palette.text)
      .child(
        h_flex()
          .flex_none()
          .items_center()
          .justify_between()
          .gap_3()
          .px_3()
          .py_2()
          .bg(palette.surface)
          .border_b_1()
          .border_color(palette.border)
          .child(
            h_flex()
              .gap_1()
              .items_center()
              .child(chip(Filter::All, cx))
              .child(chip(Filter::Slow, cx))
              .child(chip(Filter::Errors, cx))
              .child(
                Label::new(format!("{} lines · {errors} errors · {slow} slow · times are UTC", self.entries.len()))
                  .text_size(FONT_SIZE_SM)
                  .text_color(palette.text_secondary)
                  .ml_2(),
              )
              .when(self.waiting, |bar| {
                bar.child(Label::new("· new lines wait while text is selected").text_size(FONT_SIZE_SM).text_color(palette.modified))
              }),
          )
          .child(
            h_flex()
              .gap_1()
              .child(
                Button::new("activity-copy")
                  .small()
                  .label("Copy")
                  .tooltip("Copy the selection, or every line")
                  .on_click(cx.listener(|this, _, window, cx| this.copy(window, cx))),
              )
              .child(Button::new("activity-clear").small().label("Clear").on_click(cx.listener(|this, _, window, cx| this.clear(window, cx))))
              .child(
                Button::new("activity-close")
                  .small()
                  .ghost()
                  .icon(Icon::new(IconName::Close))
                  .tooltip("Close")
                  .on_click(cx.listener(|_, _, _, cx| cx.emit(ActivityClosed))),
              ),
          ),
      )
      .child(div().flex_1().min_h_0().child(Editor::new(&self.editor).readonly(true).appearance(false).bordered(false).size_full()))
  }
}
