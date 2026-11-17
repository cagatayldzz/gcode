//! Which tab the close shortcut (⌘W) closes next.

/// The shown tab goes first, then the others from the last to the first. Choosing or opening a tab starts the order again.
#[derive(Default)]
pub(crate) struct CloseOrder {
  started: bool,
}

impl CloseOrder {
  /// The index to close among `len` tabs when `active` is shown; `None` when there are no tabs.
  pub fn next(&mut self, len: usize, active: usize) -> Option<usize> {
    if len == 0 {
      self.started = false;
      return None;
    }
    let index = if self.started { len - 1 } else { active.min(len - 1) };
    self.started = true;
    Some(index)
  }

  /// The user chose or opened a tab: the next close starts with the shown one again.
  pub fn reset(&mut self) {
    self.started = false;
  }
}
