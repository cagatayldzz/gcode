//! Go to File (⌘P): a dialog that opens a file of the open project by a few letters of its path.

use super::Workspace;
use crate::{
  quick_open::{FileChosen, QuickOpen},
  GoToFile,
};
use gpui_kit::{
  component::{notification::Notification, WindowExt as _},
  *,
};

/// The width of the Go to File dialog.
const DIALOG_WIDTH: Pixels = px(560.);

impl Workspace {
  pub fn go_to_file(&mut self, _: &GoToFile, window: &mut Window, cx: &mut Context<Self>) {
    let Some(root) = self.projects.active.clone() else {
      window.push_notification(Notification::info("Open a project to go to one of its files."), cx);
      return;
    };
    let picker = cx.new(|cx| QuickOpen::new(root, window, cx));
    self._dialog_subscription = Some(cx.subscribe_in(&picker, window, |this, _, chosen: &FileChosen, window, cx| {
      window.close_dialog(cx);
      this.open_file(chosen.0.clone(), window, cx);
    }));
    let shown = picker.clone();
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Go to file").width(DIALOG_WIDTH).p_3().child(shown.clone()));
    window.defer(cx, move |window, cx| picker.update(cx, |picker, cx| picker.focus(window, cx)));
  }
}
