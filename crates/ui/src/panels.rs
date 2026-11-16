//! The panels that live in the dock: the project sidebar, the editor area and the terminal area.

use crate::workspace::Workspace;
use gpui_kit::{
  component::{
    dock::{BasePanel, Panel, PanelEvent},
    h_flex,
    label::Label,
    v_flex, ActiveTheme as _, Icon, IconName,
  },
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

  fn closable(&self, _: &App) -> bool {
    false
  }
}

impl Panel for ExplorerPanel {
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

/// Where files open. Empty until the editor exists.
pub(crate) struct EditorPanel {
  focus_handle: FocusHandle,
}

impl EditorPanel {
  pub fn new(cx: &mut Context<Self>) -> Self {
    Self { focus_handle: cx.focus_handle() }
  }
}

impl EventEmitter<PanelEvent> for EditorPanel {}

impl Focusable for EditorPanel {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus_handle.clone()
  }
}

impl BasePanel for EditorPanel {
  fn panel_name(&self) -> &'static str {
    "editor"
  }

  fn closable(&self, _: &App) -> bool {
    false
  }
}

impl Panel for EditorPanel {
  fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    "Editor"
  }
}

impl Render for EditorPanel {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    empty_state(IconName::File, "No file open", "Choose a file in the Explorer.", cx)
  }
}

/// Where the shells run. Empty until the terminal is connected.
pub(crate) struct TerminalPanel {
  focus_handle: FocusHandle,
}

impl TerminalPanel {
  pub fn new(cx: &mut Context<Self>) -> Self {
    Self { focus_handle: cx.focus_handle() }
  }
}

impl EventEmitter<PanelEvent> for TerminalPanel {}

impl Focusable for TerminalPanel {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus_handle.clone()
  }
}

impl BasePanel for TerminalPanel {
  fn panel_name(&self) -> &'static str {
    "terminal"
  }

  fn closable(&self, _: &App) -> bool {
    false
  }
}

impl Panel for TerminalPanel {
  fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    "Terminal"
  }
}

impl Render for TerminalPanel {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    empty_state(IconName::SquareTerminal, "No terminal yet", "Shells will open here.", cx)
  }
}

/// A centered icon and two lines of text for a panel with nothing to show.
fn empty_state(icon: IconName, title: &'static str, hint: &'static str, cx: &App) -> impl IntoElement {
  let muted = cx.theme().muted_foreground;
  v_flex()
    .size_full()
    .items_center()
    .justify_center()
    .gap_2()
    .child(Icon::new(icon).size_8().text_color(muted))
    .child(h_flex().child(Label::new(title).text_sm()))
    .child(Label::new(hint).text_sm().text_color(muted))
}
