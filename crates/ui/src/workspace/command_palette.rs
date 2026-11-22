//! The command palette (⇧⌘P): runs a command of the menus or opens a project from one searchable list.

use super::Workspace;
use crate::{
  command_palette::{CommandChosen, CommandPalette, PaletteCommand},
  OpenCommandPalette,
};
use gpui_kit::{component::WindowExt as _, *};

/// The width of the command palette dialog.
const DIALOG_WIDTH: Pixels = px(560.);

impl Workspace {
  pub fn open_command_palette(&mut self, _: &OpenCommandPalette, window: &mut Window, cx: &mut Context<Self>) {
    let projects = self.projects.projects.iter().map(|project| (project.path.clone(), project.name())).collect();
    let open = self.projects.active.clone();
    let palette = cx.new(|cx| CommandPalette::new(projects, open.as_deref(), window, cx));
    self._dialog_subscription = Some(cx.subscribe_in(&palette, window, |_, _, chosen: &CommandChosen, window, cx| {
      window.close_dialog(cx);
      let command = chosen.0.clone();
      // The dialog has gone and the focus is back in the window when the command runs, as if its key had been pressed.
      let workspace = cx.entity();
      window.defer(cx, move |window, cx| {
        workspace.update(cx, |this, cx| match command {
          PaletteCommand::Action(action) => this.focus_handle.dispatch_action(action.as_ref(), window, cx),
          PaletteCommand::Project(path) => this.select_project(path, window, cx),
        });
      });
    }));
    let shown = palette.clone();
    window.open_dialog(cx, move |dialog, _, _| dialog.title("Command palette").width(DIALOG_WIDTH).p_3().child(shown.clone()));
    window.defer(cx, move |window, cx| palette.update(cx, |palette, cx| palette.focus(window, cx)));
  }
}
