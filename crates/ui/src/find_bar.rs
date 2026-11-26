//! The find and replace widget at the top right of a file's editor (⌘F, ⌥⌘F). It drives the editor's own search session, so the
//! matches are highlighted by the editor itself; this view only holds the fields, the options and the buttons.

use crate::{
  components::selected_style,
  theme::{colors, FONT_SIZE_SM},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{EditorState, Input, InputEvent, InputState},
    label::Label,
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
  },
  *,
};

/// Sent when the widget was closed: the editor takes the focus back.
pub(crate) struct FindClosed;

impl EventEmitter<FindClosed> for FindBar {}

pub(crate) struct FindBar {
  editor: Entity<EditorState>,
  query: Entity<InputState>,
  replacement: Entity<InputState>,
  case_sensitive: bool,
  replace_open: bool,
  visible: bool,
  _subscriptions: Vec<Subscription>,
}

impl FindBar {
  pub fn new(editor: Entity<EditorState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
    let replacement = cx.new(|cx| InputState::new(window, cx).placeholder("Replace"));
    let subscriptions = vec![
      cx.subscribe_in(&query, window, |this, _, event: &InputEvent, window, cx| match event {
        InputEvent::Change => this.search(true, cx),
        InputEvent::PressEnter { shift, .. } => this.step(!*shift, window, cx),
        _ => {}
      }),
      cx.subscribe_in(&replacement, window, |this, _, event: &InputEvent, window, cx| {
        if let InputEvent::PressEnter { .. } = event {
          this.replace_one(window, cx);
        }
      }),
      // The match count follows the text while it is edited.
      cx.observe(&editor, |_, _, cx| cx.notify()),
    ];
    Self { editor, query, replacement, case_sensitive: false, replace_open: false, visible: false, _subscriptions: subscriptions }
  }

  pub fn is_visible(&self) -> bool {
    self.visible
  }

  /// Shows the widget with the cursor in the find field. A selection on one line becomes the query. `replace` also opens the
  /// replace row.
  pub fn open(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
    self.visible = true;
    self.replace_open |= replace;
    let selected = self.editor.read(cx).selected_text().to_string();
    if !selected.is_empty() && !selected.contains('\n') {
      self.query.update(cx, |input, cx| input.set_value(selected, window, cx));
    }
    self.query.update(cx, |input, cx| {
      window.focus(&input.focus_handle(cx), cx);
      input.select_all(window, cx);
    });
    self.search(true, cx);
    cx.notify();
  }

  pub fn close(&mut self, cx: &mut Context<Self>) {
    if !self.visible {
      return;
    }
    self.visible = false;
    self.editor.update(cx, |editor, cx| editor.close_search(cx));
    cx.emit(FindClosed);
    cx.notify();
  }

  /// Sends the query and the case option to the editor, and with `reveal` goes to the first match from the viewport.
  fn search(&mut self, reveal: bool, cx: &mut Context<Self>) {
    let query = self.query.read(cx).value().to_string();
    let case_insensitive = !self.case_sensitive;
    self.editor.update(cx, |editor, cx| {
      editor.set_search_query(query.clone(), case_insensitive, cx);
      if reveal && !query.is_empty() {
        editor.next_search_match(cx);
      }
    });
    cx.notify();
  }

  fn step(&mut self, forward: bool, _: &mut Window, cx: &mut Context<Self>) {
    self.editor.update(cx, |editor, cx| {
      if forward {
        editor.next_search_match(cx);
      } else {
        editor.previous_search_match(cx);
      }
    });
    cx.notify();
  }

  fn replace_one(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let text = self.replacement.read(cx).value().to_string();
    self.editor.update(cx, |editor, cx| editor.replace_current_search_match(&text, window, cx));
    cx.notify();
  }

  fn replace_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let text = self.replacement.read(cx).value().to_string();
    self.editor.update(cx, |editor, cx| editor.replace_all_search_matches(&text, window, cx));
    cx.notify();
  }

  /// "3 of 8", or "No results".
  fn count(&self, cx: &App) -> String {
    if self.query.read(cx).value().is_empty() {
      return String::new();
    }
    let editor = self.editor.read(cx);
    let matcher = &editor.search_session().matcher;
    match matcher.current() {
      Some(index) => format!("{} of {}", index + 1, matcher.len()),
      None => "No results".to_string(),
    }
  }

  fn field(&self, input: &Entity<InputState>, cx: &App) -> Input {
    Input::new(input).appearance(false).bordered(false).small().text_size(FONT_SIZE_SM).text_color(cx.theme().foreground)
  }
}

impl Render for FindBar {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    if !self.visible {
      return div().into_any_element();
    }
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let count = self.count(cx);
    let none = count == "No results";
    let box_style = |element: Div| element.border_1().border_color(palette.control_border).rounded_md().bg(palette.background).px_1();

    let find_row = h_flex()
      .gap_1()
      .items_center()
      .child(
        box_style(h_flex().w(rems(15.)).items_center()).child(div().flex_1().min_w_0().child(self.field(&self.query, cx))).child(
          Button::new("find-case")
            .xsmall()
            .when(self.case_sensitive, |button| button.custom(selected_style(cx)).bg(cx.theme().primary))
            .when(!self.case_sensitive, |button| button.ghost())
            .label("Aa")
            .tooltip("Match Case")
            .on_click(cx.listener(|this, _, _, cx| {
              this.case_sensitive = !this.case_sensitive;
              this.search(true, cx);
            })),
        ),
      )
      .child(
        div()
          .w(rems(5.5))
          .flex_none()
          .px_1()
          .text_size(FONT_SIZE_SM)
          .text_color(if none { theme.danger } else { theme.muted_foreground })
          .child(Label::new(count)),
      )
      .child(
        Button::new("find-previous")
          .ghost()
          .xsmall()
          .icon(Icon::new(IconName::ChevronUp))
          .tooltip("Previous Match (⇧↵)")
          .on_click(cx.listener(|this, _, window, cx| this.step(false, window, cx))),
      )
      .child(
        Button::new("find-next")
          .ghost()
          .xsmall()
          .icon(Icon::new(IconName::ChevronDown))
          .tooltip("Next Match (↵)")
          .on_click(cx.listener(|this, _, window, cx| this.step(true, window, cx))),
      )
      .child(
        Button::new("find-close")
          .ghost()
          .xsmall()
          .icon(Icon::new(IconName::Close))
          .tooltip("Close (Esc)")
          .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
      );

    let replace_row = h_flex()
      .gap_1()
      .items_center()
      .child(box_style(h_flex().w(rems(15.)).items_center()).child(div().flex_1().min_w_0().child(self.field(&self.replacement, cx))))
      .child(
        Button::new("replace-one")
          .ghost()
          .xsmall()
          .label("Replace")
          .tooltip("Replace (↵)")
          .on_click(cx.listener(|this, _, window, cx| this.replace_one(window, cx))),
      )
      .child(
        Button::new("replace-all")
          .ghost()
          .xsmall()
          .label("All")
          .tooltip("Replace All")
          .on_click(cx.listener(|this, _, window, cx| this.replace_all(window, cx))),
      );

    h_flex()
      .id("find-bar")
      .absolute()
      .top_2()
      .right_6()
      .items_start()
      .gap_1()
      .p_1()
      .rounded_md()
      .border_1()
      .border_color(palette.border)
      .bg(palette.surface)
      .shadow_md()
      .text_size(FONT_SIZE_SM)
      .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
        if event.keystroke.key == "escape" {
          this.close(cx);
          cx.stop_propagation();
        }
      }))
      .child(
        Button::new("find-toggle-replace")
          .ghost()
          .xsmall()
          .icon(Icon::new(if self.replace_open { IconName::ChevronDown } else { IconName::ChevronRight }))
          .tooltip("Toggle Replace")
          .on_click(cx.listener(|this, _, _, cx| {
            this.replace_open = !this.replace_open;
            cx.notify();
          })),
      )
      .child(v_flex().gap_1().child(find_row).when(self.replace_open, |column| column.child(replace_row)))
      .into_any_element()
  }
}
