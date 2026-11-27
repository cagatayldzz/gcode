#[path = "../src/agents.rs"]
mod agents;

use agents::{AgentKind, AgentTabs};

fn sessions(tabs: &AgentTabs<u32>) -> Vec<(AgentKind, u32)> {
  tabs.tabs.iter().map(|tab| (tab.kind, tab.session)).collect()
}

#[test]
fn a_new_session_is_shown() {
  let mut tabs = AgentTabs::new();
  tabs.add(AgentKind::Codex, 1, None);
  tabs.add(AgentKind::Claude, 2, None);
  assert_eq!(sessions(&tabs), [(AgentKind::Codex, 1), (AgentKind::Claude, 2)]);
  assert_eq!(tabs.current().map(|tab| tab.session), Some(2));
}

#[test]
fn selecting_a_missing_tab_changes_nothing() {
  let mut tabs = AgentTabs::new();
  tabs.add(AgentKind::Codex, 1, None);
  tabs.select(5);
  assert_eq!(tabs.active, 0);
}

#[test]
fn closing_the_shown_session_shows_its_left_neighbor() {
  let mut tabs = AgentTabs::new();
  for session in 1..=3 {
    tabs.add(AgentKind::Codex, session, None);
  }
  assert!(tabs.remove(&3));
  assert_eq!(tabs.current().map(|tab| tab.session), Some(2));
}

#[test]
fn closing_an_earlier_session_keeps_the_shown_one() {
  let mut tabs = AgentTabs::new();
  for session in 1..=3 {
    tabs.add(AgentKind::Claude, session, None);
  }
  assert!(tabs.remove(&1));
  assert_eq!(tabs.current().map(|tab| tab.session), Some(3));
}

#[test]
fn closing_the_first_shown_session_shows_the_next() {
  let mut tabs = AgentTabs::new();
  tabs.add(AgentKind::Codex, 1, None);
  tabs.add(AgentKind::Codex, 2, None);
  tabs.select(0);
  assert!(tabs.remove(&1));
  assert_eq!(tabs.current().map(|tab| tab.session), Some(2));
}

#[test]
fn removing_an_unknown_session_reports_it() {
  let mut tabs: AgentTabs<u32> = AgentTabs::new();
  assert!(!tabs.remove(&9));
  assert!(tabs.is_empty());
}

#[test]
fn the_shell_line_runs_the_agent_and_explains_a_missing_one() {
  let line = AgentKind::Claude.shell_line();
  assert!(line.contains("exec claude"));
  assert!(line.contains("claude was not found"));
}

#[test]
fn every_agent_has_a_distinct_name_command_and_icon() {
  let all = AgentKind::ALL;
  for (index, kind) in all.iter().enumerate() {
    assert!(!kind.label().is_empty() && !kind.description().is_empty());
    assert!(kind.icon().starts_with("icons/") && kind.icon().ends_with(".svg"));
    for other in &all[index + 1..] {
      assert_ne!(kind.label(), other.label());
      assert_ne!(kind.command(), other.command());
      assert_ne!(kind.icon(), other.icon());
    }
  }
}

#[test]
fn every_agent_has_the_plain_id_an_extension_manifest_names_it_by() {
  let ids: Vec<&str> = AgentKind::ALL.iter().map(|kind| kind.id()).collect();
  assert_eq!(ids, ["codex", "claude"]);
}
