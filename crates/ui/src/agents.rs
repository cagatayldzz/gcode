//! The AI agents the app opens in their own view, and how a project's agent sessions are arranged: tabs, one session each.
//! Pure data, so that the rules (which tab opens next, which one takes a closed tab's place) can be tested without a window.

use std::path::PathBuf;

/// A command-line agent that the app can start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentKind {
  Codex,
  Claude,
}

impl AgentKind {
  pub const ALL: [AgentKind; 2] = [AgentKind::Codex, AgentKind::Claude];

  /// The id an extension's manifest names the agent by.
  pub fn id(self) -> &'static str {
    match self {
      AgentKind::Codex => "codex",
      AgentKind::Claude => "claude",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      AgentKind::Codex => "Codex",
      AgentKind::Claude => "Claude",
    }
  }

  /// The program that starts the agent.
  pub fn command(self) -> &'static str {
    match self {
      AgentKind::Codex => "codex",
      AgentKind::Claude => "claude",
    }
  }

  pub fn description(self) -> &'static str {
    match self {
      AgentKind::Codex => "OpenAI's coding agent",
      AgentKind::Claude => "Anthropic's coding agent",
    }
  }

  pub fn icon(self) -> &'static str {
    match self {
      AgentKind::Codex => "icons/bot.svg",
      AgentKind::Claude => "icons/sparkles.svg",
    }
  }

  /// The shell line that runs the agent. A shell that is not found says so and waits for Enter, so the view does not close
  /// before the message can be read.
  pub fn shell_line(self) -> String {
    let command = self.command();
    format!("if command -v {command} >/dev/null 2>&1; then exec {command}; else printf '%s\\n' '{command} was not found in your PATH. Install it, then open it again.'; read -r _; fi")
  }
}

/// One agent session: which agent runs, and the terminal that shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentTab<T> {
  pub kind: AgentKind,
  pub session: T,
  pub worktree: Option<PathBuf>,
}

/// The agent sessions of one project, as tabs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentTabs<T> {
  pub tabs: Vec<AgentTab<T>>,
  pub active: usize,
}

impl<T: PartialEq> AgentTabs<T> {
  pub fn new() -> Self {
    Self { tabs: Vec::new(), active: 0 }
  }

  pub fn is_empty(&self) -> bool {
    self.tabs.is_empty()
  }

  /// Adds a session after the others and shows it.
  pub fn add(&mut self, kind: AgentKind, session: T, worktree: Option<PathBuf>) {
    self.tabs.push(AgentTab { kind, session, worktree });
    self.active = self.tabs.len() - 1;
  }

  /// The shown session.
  pub fn current(&self) -> Option<&AgentTab<T>> {
    self.tabs.get(self.active)
  }

  /// Shows the tab at `index`, if there is one.
  pub fn select(&mut self, index: usize) {
    if index < self.tabs.len() {
      self.active = index;
    }
  }

  /// Takes a session out. The tab that was shown stays shown; when it is the one that goes, its neighbor on the left takes
  /// its place (the first, when it was the first). `false` when the session is not here.
  pub fn remove(&mut self, session: &T) -> bool {
    let Some(index) = self.tabs.iter().position(|tab| &tab.session == session) else { return false };
    self.tabs.remove(index);
    if index < self.active || (index == self.active && index > 0) {
      self.active -= 1;
    }
    true
  }
}
