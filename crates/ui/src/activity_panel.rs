//! The activity log panel at the bottom of the window, opened by the debug button of the title bar: what the app did (Git commands, requests to services, steps
//! of the interface) with how long each took and how it ended, newest first, kept up to date while it is open. Reads `ws_activity`.

use crate::theme::{colors, FONT_SIZE_SM};
use gpui_kit::{base::Selectable as _, prelude::FluentBuilder as _};
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    empty::{Empty, EmptyDescription, EmptyHeader, EmptyTitle},
    h_flex,
    label::Label,
    notification::Notification,
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _,
  },
  *,
};
use std::time::Duration;
use ws_activity::{Entry, Level};

/// How often the window looks for new lines.
const POLL: Duration = Duration::from_millis(300);

/// The most lines shown at once; the window says when older ones are left out.
const SHOWN: usize = 600;

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
  scroll: ScrollHandle,
  _poll: Task<()>,
}

impl ActivityLog {
  pub(crate) fn new(cx: &mut Context<Self>) -> Self {
    let poll = cx.spawn(async move |this, cx| loop {
      cx.background_executor().timer(POLL).await;
      if this.update(cx, |this, cx| this.refresh(cx)).is_err() {
        break;
      }
    });
    let scroll = ScrollHandle::new();
    scroll.scroll_to_bottom();
    Self { entries: ws_activity::entries(), seen: ws_activity::version(), filter: Filter::All, scroll, _poll: poll }
  }

  /// Takes the new lines, when there are any.
  fn refresh(&mut self, cx: &mut Context<Self>) {
    let version = ws_activity::version();
    if version != self.seen {
      // New lines come at the bottom; the list follows them unless the user scrolled up to read.
      let follow = self.scroll.max_offset().y <= px(1.) || self.scroll.offset().y <= -self.scroll.max_offset().y + px(4.);
      self.seen = version;
      self.entries = ws_activity::entries();
      if follow {
        self.scroll.scroll_to_bottom();
      }
      cx.notify();
    }
  }

  fn set_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
    self.filter = filter;
    self.scroll.scroll_to_bottom();
    cx.notify();
  }

  fn clear(&mut self, cx: &mut Context<Self>) {
    ws_activity::clear();
    self.refresh(cx);
  }

  /// The lines the filter lets through, as text, oldest first.
  fn copy(&self, window: &mut Window, cx: &mut Context<Self>) {
    let text: Vec<String> = self.entries.iter().filter(|entry| self.filter.matches(entry)).map(Entry::text).collect();
    let count = text.len();
    cx.write_to_clipboard(ClipboardItem::new_string(text.join("\n")));
    window.push_notification(Notification::info(format!("Copied {count} log {}.", if count == 1 { "line" } else { "lines" })), cx);
  }
}

impl Render for ActivityLog {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let palette = colors(cx);
    let theme = cx.theme().clone();
    let matching: Vec<&Entry> = self.entries.iter().filter(|entry| self.filter.matches(entry)).collect();
    let (errors, slow) = self.entries.iter().fold((0, 0), |(errors, slow), entry| match entry.level {
      Level::Error => (errors + 1, slow),
      Level::Slow => (errors, slow + 1),
      Level::Info => (errors, slow),
    });
    let hidden = matching.len().saturating_sub(SHOWN);
    let rows: Vec<AnyElement> = matching
      .into_iter()
      .skip(hidden)
      .map(|entry| {
        let level = match entry.level {
          Level::Error => palette.deleted,
          Level::Slow => palette.modified,
          Level::Info => palette.text_secondary,
        };
        h_flex()
          .id(("activity-line", entry.id))
          .w_full()
          .items_start()
          .gap_3()
          .px_3()
          .py_0p5()
          .when(entry.level == Level::Error, |row| row.bg(palette.diff_removed))
          .font_family(theme.mono_font_family.clone())
          .text_size(FONT_SIZE_SM)
          .child(div().flex_none().text_color(palette.text_secondary).child(SharedString::from(entry.time.clone())))
          .child(div().flex_none().w_12().text_color(level).child(entry.level.label()))
          .child(div().flex_none().w_16().text_color(palette.text_secondary).child(SharedString::from(format!("[{}]", entry.category))))
          .child(div().flex_1().min_w_0().text_color(palette.text).child(SharedString::from(entry.message.clone())))
          .into_any_element()
      })
      .collect();
    let body = if rows.is_empty() {
      Empty::new()
        .flex_1()
        .min_h_0()
        .child(
          EmptyHeader::new()
            .title(EmptyTitle::new().child("Nothing logged yet"))
            .description(EmptyDescription::new().child("Git commands, requests to services and failures show up here as they happen.")),
        )
        .into_any_element()
    } else {
      // The scrollbar sits on a wrapper, not on the scrolled list: inside the list it would scroll away with the lines.
      div()
        .flex_1()
        .min_h_0()
        .relative()
        .child(
          div().id("activity-lines").size_full().overflow_y_scroll().track_scroll(&self.scroll).child(
            v_flex()
              .w_full()
              .when(hidden > 0, |list| {
                list.child(
                  Label::new(format!("{hidden} older lines are not shown. Copy has all of them."))
                    .text_size(FONT_SIZE_SM)
                    .text_color(palette.text_secondary)
                    .p_3(),
                )
              })
              .children(rows),
          ),
        )
        .vertical_scrollbar(&self.scroll)
        .into_any_element()
    };
    let chip = |filter: Filter, cx: &mut Context<Self>| {
      Button::new(filter.label())
        .small()
        .ghost()
        .label(filter.label())
        .selected(self.filter == filter)
        .on_click(cx.listener(move |this, _, _, cx| this.set_filter(filter, cx)))
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
            h_flex().gap_1().items_center().child(chip(Filter::All, cx)).child(chip(Filter::Slow, cx)).child(chip(Filter::Errors, cx)).child(
              Label::new(format!("{} lines · {errors} errors · {slow} slow · times are UTC", self.entries.len()))
                .text_size(FONT_SIZE_SM)
                .text_color(palette.text_secondary)
                .ml_2(),
            ),
          )
          .child(
            h_flex()
              .gap_1()
              .child(Button::new("activity-copy").small().label("Copy").on_click(cx.listener(|this, _, window, cx| this.copy(window, cx))))
              .child(Button::new("activity-clear").small().label("Clear").on_click(cx.listener(|this, _, _, cx| this.clear(cx))))
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
      .child(body)
  }
}
