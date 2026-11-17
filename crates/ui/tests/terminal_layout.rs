#[path = "../src/terminal_layout.rs"]
mod terminal_layout;

use terminal_layout::{Arrangement, Direction, Node};

fn panes(arrangement: &Arrangement<u32>) -> Vec<Vec<u32>> {
  arrangement.tabs.iter().map(|tab| tab.panes().into_iter().copied().collect()).collect()
}

#[test]
fn a_new_tab_is_shown() {
  let mut tabs = Arrangement::new();
  assert!(tabs.is_empty() && tabs.current().is_none());
  tabs.add_tab(1);
  tabs.add_tab(2);
  assert_eq!((panes(&tabs), tabs.active), (vec![vec![1], vec![2]], 1));
  tabs.select(0);
  tabs.select(9);
  assert_eq!(tabs.active, 0, "an unknown tab is ignored");
}

#[test]
fn splitting_puts_the_new_pane_after_the_old_one_and_shows_its_tab() {
  let mut tabs = Arrangement::new();
  tabs.add_tab(1);
  tabs.add_tab(2);
  tabs.select(1);
  assert!(tabs.split(&1, 3, Direction::Columns));
  assert_eq!((panes(&tabs), tabs.active), (vec![vec![1, 3], vec![2]], 0));
  assert!(tabs.split(&3, 4, Direction::Rows));
  assert_eq!(panes(&tabs)[0], vec![1, 3, 4]);
  match tabs.current().unwrap() {
    Node::Split { direction, second, .. } => {
      assert_eq!(*direction, Direction::Columns);
      assert!(matches!(**second, Node::Split { direction: Direction::Rows, .. }), "the second part was split again, one above the other");
    }
    Node::Pane(_) => panic!("the tab was split"),
  }
  assert!(!tabs.split(&99, 5, Direction::Columns), "a pane that is nowhere cannot be split");
}

#[test]
fn every_split_has_its_own_id() {
  let mut tabs = Arrangement::new();
  tabs.add_tab(1);
  tabs.split(&1, 2, Direction::Columns);
  tabs.split(&2, 3, Direction::Rows);
  let Node::Split { id: outer, second, .. } = tabs.current().unwrap() else { panic!() };
  let Node::Split { id: inner, .. } = **second else { panic!() };
  assert_ne!(*outer, inner);
}

#[test]
fn closing_a_pane_lets_its_sibling_take_the_whole_place() {
  let mut tabs = Arrangement::new();
  tabs.add_tab(1);
  tabs.split(&1, 2, Direction::Columns);
  tabs.split(&2, 3, Direction::Rows);
  tabs.remove(&2);
  assert_eq!(panes(&tabs), vec![vec![1, 3]]);
  tabs.remove(&1);
  assert_eq!(panes(&tabs), vec![vec![3]]);
  assert!(matches!(tabs.current().unwrap(), Node::Pane(3)), "a lone pane is not a split any more");
}

#[test]
fn closing_the_last_pane_of_a_tab_closes_the_tab_and_shows_the_one_beside_it() {
  let mut tabs = Arrangement::new();
  for pane in [1, 2, 3] {
    tabs.add_tab(pane);
  }
  tabs.select(1);
  tabs.remove(&2);
  assert_eq!((panes(&tabs), tabs.active), (vec![vec![1], vec![3]], 1), "the tab on the right is shown");
  tabs.remove(&3);
  assert_eq!((panes(&tabs), tabs.active), (vec![vec![1]], 0), "else the one on the left");
  tabs.remove(&1);
  assert!(tabs.is_empty());
  tabs.remove(&1);
}

#[test]
fn closing_a_pane_in_another_tab_keeps_the_shown_tab() {
  let mut tabs = Arrangement::new();
  for pane in [1, 2, 3] {
    tabs.add_tab(pane);
  }
  assert_eq!(tabs.active, 2);
  tabs.remove(&1);
  assert_eq!((panes(&tabs), tabs.active), (vec![vec![2], vec![3]], 1));
  assert_eq!(tabs.panes().into_iter().copied().collect::<Vec<_>>(), vec![2, 3]);
}
