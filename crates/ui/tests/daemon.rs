#[path = "../src/daemon.rs"]
#[allow(dead_code)]
mod daemon;

use std::path::{Path, PathBuf};

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
