//! The project picker of the AI view is driven from the keyboard: arrows move the highlight, Enter inserts, Shift+Enter ticks.

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use ws_ui::{MentionChoice, MentionPicker, ProjectsChosen};

/// What the picker handed over so far, one list of paths per insert.
type Inserted = Rc<RefCell<Vec<Vec<PathBuf>>>>;

fn picker(cx: &mut TestAppContext) -> (Entity<MentionPicker>, &mut VisualTestContext, Inserted) {
  cx.update(|cx| {
    gpui_kit::init(cx);
    ws_ui::init(cx);
  });
  let choices = ["alpha", "beta", "gamma"]
    .into_iter()
    .map(|name| MentionChoice { path: PathBuf::from(format!("/work/{name}")), name: name.into(), folder: format!("~/work/{name}").into() })
    .collect();
  let (picker, window) = cx.add_window_view(|window, cx| MentionPicker::new(choices, window, cx));
  let chosen = Rc::new(RefCell::new(Vec::new()));
  let sink = chosen.clone();
  window.update(|window, cx| {
    cx.subscribe(&picker, move |_, event: &ProjectsChosen, _| sink.borrow_mut().push(event.0.clone())).detach();
    let focus = picker.read(cx).focus_handle(cx);
    focus.focus(window, cx);
  });
  window.run_until_parked();
  (picker, window, chosen)
}

#[gpui_kit::test]
fn enter_inserts_the_first_project_when_nothing_was_moved(cx: &mut TestAppContext) {
  let (_picker, window, chosen) = picker(cx);
  window.simulate_keystrokes("enter");
  assert_eq!(*chosen.borrow(), vec![vec![PathBuf::from("/work/alpha")]]);
}

#[gpui_kit::test]
fn the_arrows_move_to_a_project_and_enter_inserts_it(cx: &mut TestAppContext) {
  let (_picker, window, chosen) = picker(cx);
  window.simulate_keystrokes("down down up");
  window.simulate_keystrokes("enter");
  assert_eq!(*chosen.borrow(), vec![vec![PathBuf::from("/work/beta")]]);
}

#[gpui_kit::test]
fn the_highlight_stops_at_the_ends_of_the_list(cx: &mut TestAppContext) {
  let (_picker, window, chosen) = picker(cx);
  window.simulate_keystrokes("down down down down enter");
  assert_eq!(*chosen.borrow(), vec![vec![PathBuf::from("/work/gamma")]]);
}

#[gpui_kit::test]
fn shift_enter_ticks_projects_and_enter_inserts_the_ticked_ones(cx: &mut TestAppContext) {
  let (_picker, window, chosen) = picker(cx);
  window.simulate_keystrokes("shift-enter down down shift-enter up enter");
  assert_eq!(*chosen.borrow(), vec![vec![PathBuf::from("/work/alpha"), PathBuf::from("/work/gamma")]]);
}

#[gpui_kit::test]
fn typing_filters_the_list_and_puts_the_highlight_on_the_first_match(cx: &mut TestAppContext) {
  let (_picker, window, chosen) = picker(cx);
  window.simulate_keystrokes("down down");
  window.simulate_input("bet");
  window.simulate_keystrokes("enter");
  assert_eq!(*chosen.borrow(), vec![vec![PathBuf::from("/work/beta")]]);
}
