#[path = "../src/daemon.rs"]
#[allow(dead_code)]
mod daemon;

use std::{
  path::{Path, PathBuf},
  time::{Duration, Instant},
};

#[test]
fn the_managed_codex_is_tried_before_the_one_on_the_path() {
  let candidates = daemon::codex_candidates(Some(Path::new("/home/someone")));
  assert_eq!(candidates, vec![PathBuf::from("/home/someone/.codex/packages/app-server-daemon/current/bin/codex"), PathBuf::from("codex")]);
}

#[test]
fn without_a_home_only_the_path_is_searched() {
  assert_eq!(daemon::codex_candidates(None), vec![PathBuf::from("codex")]);
}

#[test]
fn a_failure_names_the_exit_code_and_the_last_line_of_the_error() {
  let message = daemon::describe_failure(Some(2), "first\nlast problem\n\n");
  assert_eq!(message, "Could not restart the Codex daemon (exit code 2): last problem");
  assert_eq!(daemon::describe_failure(None, ""), "Could not restart the Codex daemon: no details");
}

#[test]
fn a_command_that_hangs_is_killed_at_the_timeout() {
  let started = Instant::now();
  let outcome = daemon::run_with_timeout(Path::new("sleep"), &["30"], Duration::from_millis(200)).unwrap();
  assert!(matches!(outcome, daemon::Outcome::TimedOut));
  assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_command_that_ends_reports_its_exit_code_and_error_output() {
  let outcome = daemon::run_with_timeout(Path::new("sh"), &["-c", "echo broken >&2; exit 3"], Duration::from_secs(5)).unwrap();
  match outcome {
    daemon::Outcome::Finished { code, stderr } => {
      assert_eq!(code, Some(3));
      assert_eq!(stderr.trim(), "broken");
    }
    daemon::Outcome::TimedOut => panic!("the command ended on its own"),
  }
}

#[test]
fn the_process_pattern_only_matches_the_managed_daemon() {
  let pattern = regex_lite_free_match;
  assert!(pattern("/h/.codex/packages/app-server-daemon/releases/0.171.0/bin/codex app-server --listen"));
  assert!(pattern("/h/.codex/packages/app-server-daemon/current/bin/codex app-server daemon restart"));
  assert!(!pattern("codex"));
  assert!(!pattern("/usr/local/bin/codex app-server"));
}

/// Mirrors `DAEMON_PROCESS_PATTERN` with plain string checks, so the test needs no regex crate.
fn regex_lite_free_match(command: &str) -> bool {
  assert_eq!(daemon::DAEMON_PROCESS_PATTERN, "app-server-daemon/.*(codex app-server|codex-code-mode-host)");
  command.find("app-server-daemon/").is_some_and(|at| command[at..].contains("codex app-server") || command[at..].contains("codex-code-mode-host"))
}
