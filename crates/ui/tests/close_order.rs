#[path = "../src/close_order.rs"]
mod close_order;

use close_order::CloseOrder;

/// Closes tabs the way the editor does (the neighbour on the right, else on the left, is shown) and returns their names in order.
fn close_all(tabs: &mut Vec<char>, mut active: usize, order: &mut CloseOrder) -> Vec<char> {
  let mut closed = Vec::new();
  while let Some(index) = order.next(tabs.len(), active) {
    closed.push(tabs.remove(index));
    if active > index || active >= tabs.len() {
      active = active.saturating_sub(1);
    }
  }
  closed
}

#[test]
fn the_shown_tab_closes_first_then_the_rest_from_the_last_to_the_first() {
  let mut tabs = vec!['a', 'b', 'c', 'd'];
  assert_eq!(close_all(&mut tabs, 1, &mut CloseOrder::default()), vec!['b', 'd', 'c', 'a']);
  assert!(tabs.is_empty());
}

#[test]
fn closing_the_last_shown_tab_continues_from_the_new_last_one() {
  let mut tabs = vec!['a', 'b', 'c'];
  assert_eq!(close_all(&mut tabs, 2, &mut CloseOrder::default()), vec!['c', 'b', 'a']);
}

#[test]
fn choosing_a_tab_starts_the_order_again() {
  let mut order = CloseOrder::default();
  assert_eq!(order.next(4, 0), Some(0));
  assert_eq!(order.next(3, 0), Some(2));
  order.reset();
  assert_eq!(order.next(3, 1), Some(1));
}

#[test]
fn no_tabs_means_nothing_to_close() {
  let mut order = CloseOrder::default();
  assert_eq!(order.next(0, 0), None);
  assert_eq!(order.next(1, 5), Some(0));
}
