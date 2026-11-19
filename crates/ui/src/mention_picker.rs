//! The list of projects that opens when `$$` is typed in an AI agent: tick the projects to mention, then insert their paths.

use crate::theme::{colors, FONT_SIZE_SM};
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{InputEvent, InputState},
    label::Label,
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme as _, Sizable as _,
  },
  *,
};
use std::{collections::HashSet, path::PathBuf};

/// The tallest the list gets before it scrolls.
const LIST_MAX_HEIGHT: Pixels = px(260.);

/// A project that can be mentioned.
pub(crate) struct MentionChoice {
  pub path: PathBuf,
  pub name: SharedString,
  pub folder: SharedString,
}

/// Sent with the paths of the projects the user ticked, in the order of the list.
pub(crate) struct ProjectsChosen(pub Vec<PathBuf>);

impl EventEmitter<ProjectsChosen> for MentionPicker {}

pub(crate) struct MentionPicker {
  choices: Vec<MentionChoice>,
  ticked: HashSet<PathBuf>,
  filter: Entity<InputState>,
  _subscription: Subscription,
}

impl MentionPicker {
  pub fn new(choices: Vec<MentionChoice>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a project to mention"));
    let subscription = cx.subscribe(&filter, |this, _, event: &InputEvent, cx| match event {
      InputEvent::PressEnter { .. } => this.confirm(cx),
      _ => cx.notify(),
    });
    Self { choices, ticked: HashSet::new(), filter, _subscription: subscription }
  }

  pub fn focus_handle(&self, cx: &App) -> FocusHandle {
    self.filter.read(cx).focus_handle(cx)
  }

  fn toggle(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
    if !self.ticked.remove(path) {
      self.ticked.insert(path.clone());
    }
    cx.notify();
  }

  /// Hands over the ticked projects; with none ticked, the only project the filter leaves.
  fn confirm(&mut self, cx: &mut Context<Self>) {
    let query = self.filter.read(cx).value().to_lowercase();
    let mut chosen: Vec<PathBuf> =
      self.choices.iter().filter(|choice| self.ticked.contains(&choice.path)).map(|choice| choice.path.clone()).collect();
    if chosen.is_empty() {
      let mut matching = self.choices.iter().filter(|choice| choice.name.to_lowercase().contains(&query));
      if let (Some(only), None) = (matching.next(), matching.next()) {
        chosen.push(only.path.clone());
      }
    }
    if !chosen.is_empty() {
      cx.emit(ProjectsChosen(chosen));
    }
  }
}

impl Render for MentionPicker {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let palette = colors(cx);
    let query = self.filter.read(cx).value().to_lowercase();
    let rows: Vec<AnyElement> = self
      .choices
      .iter()
      .filter(|choice| query.is_empty() || choice.name.to_lowercase().contains(&query))
      .map(|choice| {
        let path = choice.path.clone();
        let key = choice.path.display();
        let toggled = path.clone();
        h_flex()
          .id(SharedString::from(format!("mention-{key}")))
          .gap_2()
          .px_2()
          .py_1()
          .rounded_md()
          .items_center()
          .cursor_pointer()
          .hover(|row| row.bg(palette.hover))
          .child(Checkbox::new(SharedString::from(format!("mention-check-{key}"))).checked(self.ticked.contains(&path)))
          .child(
            v_flex().flex_1().min_w_0().child(Label::new(choice.name.clone()).overflow_hidden().whitespace_nowrap().text_ellipsis()).child(
              Label::new(choice.folder.clone())
                .text_size(FONT_SIZE_SM)
                .text_color(theme.muted_foreground)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis(),
            ),
          )
          .on_click(cx.listener(move |this, _, _, cx| this.toggle(&toggled, cx)))
          .into_any_element()
      })
      .collect();
    let count = self.ticked.len();
    v_flex()
      .gap_2()
      .child(crate::components::soft_input(&self.filter, cx))
      .child(if rows.is_empty() {
        Label::new("No project matches.").text_color(theme.muted_foreground).p_2().into_any_element()
      } else {
        v_flex().id("mention-list").max_h(LIST_MAX_HEIGHT).overflow_y_scrollbar().gap_0p5().children(rows).into_any_element()
      })
      .child(
        h_flex()
          .justify_between()
          .items_center()
          .child(Label::new("Tick the projects to mention, then press Enter.").text_size(FONT_SIZE_SM).text_color(theme.muted_foreground))
          .child(
            Button::new("mention-insert")
              .small()
              .primary()
              .label(if count > 0 { format!("Insert {count}") } else { "Insert".to_owned() })
              .on_click(cx.listener(|this, _, _, cx| this.confirm(cx))),
          ),
      )
  }
}
