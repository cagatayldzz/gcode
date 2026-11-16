//! The sidebar panel of the dock.

use crate::workspace::Workspace;
use gpui_kit::{
  component::dock::{BasePanel, Panel, PanelControl, PanelEvent},
  *,
};

/// The project list, the repository summary and the folder tree. Draws what the workspace holds.
pub(crate) struct ExplorerPanel {
  workspace: WeakEntity<Workspace>,
  focus_handle: FocusHandle,
}

impl ExplorerPanel {
  pub fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
    Self { workspace, focus_handle: cx.focus_handle() }
  }
}

impl EventEmitter<PanelEvent> for ExplorerPanel {}

impl Focusable for ExplorerPanel {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus_handle.clone()
  }
}

impl BasePanel for ExplorerPanel {
  fn panel_name(&self) -> &'static str {
    "explorer"
  }

  fn zoomable(&self, _: &App) -> bool {
    false
  }

  fn closable(&self, _: &App) -> bool {
    false
  }
}

impl Panel for ExplorerPanel {
  fn zoom_control(&self, _: &App) -> Option<PanelControl> {
    None
  }

  fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    "Explorer"
  }
}

impl Render for ExplorerPanel {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    match self.workspace.update(cx, |workspace, cx| workspace.render_sidebar(cx)) {
      Ok(sidebar) => sidebar,
      Err(_) => div().into_any_element(),
    }
  }
}
