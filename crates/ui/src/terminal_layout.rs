//! How the terminals of a project are arranged: tabs, each holding panes that are split side by side or one above the other.
//! Pure data, so that the rules (which pane takes a closed pane's place, which tab opens next) can be tested without a window.

/// How a split lays out its two parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
  /// Side by side.
  Columns,
  /// One above the other.
  Rows,
}

/// A tab: one pane, or two parts split by a direction, each of which is again a pane or a split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Node<T> {
  Pane(T),
  Split {
    /// Names the split, so that its divider keeps its place while the others change.
    id: u64,
    direction: Direction,
    first: Box<Node<T>>,
    second: Box<Node<T>>,
  },
}

impl<T: PartialEq + Clone> Node<T> {
  /// The panes, left to right and top to bottom.
  pub fn panes(&self) -> Vec<&T> {
    match self {
      Node::Pane(pane) => vec![pane],
      Node::Split { first, second, .. } => first.panes().into_iter().chain(second.panes()).collect(),
    }
  }

  pub fn contains(&self, target: &T) -> bool {
    self.panes().into_iter().any(|pane| pane == target)
  }

  /// Splits `target`: it and `new` share its place, `new` after it. `false` when `target` is not here.
  fn split(&mut self, target: &T, new: T, direction: Direction, id: u64) -> bool {
    match self {
      Node::Pane(pane) if pane == target => {
        let old = std::mem::replace(self, Node::Pane(new.clone()));
        *self = Node::Split { id, direction, first: Box::new(old), second: Box::new(Node::Pane(new)) };
        true
      }
      Node::Pane(_) => false,
      Node::Split { first, second, .. } => first.split(target, new.clone(), direction, id) || second.split(target, new, direction, id),
    }
  }

  /// Puts `new` in the place of `target`. `false` when `target` is not here.
  fn replace(&mut self, target: &T, new: T) -> bool {
    match self {
      Node::Pane(pane) if pane == target => {
        *pane = new;
        true
      }
      Node::Pane(_) => false,
      Node::Split { first, second, .. } => first.replace(target, new.clone()) || second.replace(target, new),
    }
  }

  /// Takes `target` out; its sibling takes the place of the split. `None` when it was the only pane.
  fn without(self, target: &T) -> Option<Node<T>> {
    match self {
      Node::Pane(pane) => (pane != *target).then_some(Node::Pane(pane)),
      Node::Split { id, direction, first, second } => match (first.without(target), second.without(target)) {
        (Some(first), Some(second)) => Some(Node::Split { id, direction, first: Box::new(first), second: Box::new(second) }),
        (only, None) | (None, only) => only,
      },
    }
  }
}

/// The tabs of one project and the one that is shown.
#[derive(Clone, Debug)]
pub(crate) struct Arrangement<T> {
  pub tabs: Vec<Node<T>>,
  pub active: usize,
  next_id: u64,
}

impl<T: PartialEq + Clone> Arrangement<T> {
  pub fn new() -> Self {
    Self { tabs: Vec::new(), active: 0, next_id: 0 }
  }

  pub fn is_empty(&self) -> bool {
    self.tabs.is_empty()
  }

  /// The tab that is shown.
  pub fn current(&self) -> Option<&Node<T>> {
    self.tabs.get(self.active)
  }

  /// Every pane of every tab.
  pub fn panes(&self) -> Vec<&T> {
    self.tabs.iter().flat_map(Node::panes).collect()
  }

  /// Opens a tab with one pane and shows it.
  pub fn add_tab(&mut self, pane: T) {
    self.tabs.push(Node::Pane(pane));
    self.active = self.tabs.len() - 1;
  }

  /// Shows another tab.
  pub fn select(&mut self, index: usize) {
    if index < self.tabs.len() {
      self.active = index;
    }
  }

  /// Splits `target` in two with `new` after it, and shows its tab. `false` when `target` is in no tab.
  pub fn split(&mut self, target: &T, new: T, direction: Direction) -> bool {
    let Some(index) = self.tabs.iter().position(|tab| tab.contains(target)) else { return false };
    self.next_id += 1;
    let id = self.next_id;
    self.active = index;
    self.tabs[index].split(target, new, direction, id)
  }

  /// Puts `new` in the place of `target`, in the same tab and split, and shows that tab. `false` when `target` is in no tab.
  pub fn replace(&mut self, target: &T, new: T) -> bool {
    let Some(index) = self.tabs.iter().position(|tab| tab.contains(target)) else { return false };
    self.active = index;
    self.tabs[index].replace(target, new)
  }

  /// Takes a pane out. A tab left without panes goes too, and the tab on its right (else on its left) is shown.
  pub fn remove(&mut self, target: &T) {
    let Some(index) = self.tabs.iter().position(|tab| tab.contains(target)) else { return };
    let tab = self.tabs.remove(index);
    match tab.without(target) {
      Some(rest) => self.tabs.insert(index, rest),
      None => {
        if self.active > index || self.active >= self.tabs.len() {
          self.active = self.active.saturating_sub(1);
        }
        return;
      }
    }
    self.active = self.active.min(self.tabs.len() - 1);
  }
}
