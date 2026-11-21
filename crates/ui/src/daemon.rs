//! Restarting the Codex app-server daemon: it can stay up for days and stop answering, and Codex then shows "Reconnecting to server…".

use std::{
  path::{Path, PathBuf},
  process::Command,
};

/// The `codex` programs to try, the one Codex manages for its daemon first, then the one on the `PATH`.
pub(crate) fn codex_candidates(home: Option<&Path>) -> Vec<PathBuf> {
  let managed = home.map(|home| home.join(".codex/packages/app-server-daemon/current/bin/codex"));
  managed.into_iter().chain([PathBuf::from("codex")]).collect()
}

/// What to tell the user when the restart ended with `status` and `stderr`.
pub(crate) fn describe_failure(code: Option<i32>, stderr: &str) -> String {
  let detail = stderr.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("no details").trim();
  match code {
    Some(code) => format!("Could not restart the Codex daemon (exit code {code}): {detail}"),
    None => format!("Could not restart the Codex daemon: {detail}"),
  }
}

/// Runs `codex app-server daemon restart`. Blocks until it ends, so call it off the interface thread.
pub(crate) fn restart_codex_daemon() -> Result<(), String> {
  let home = std::env::var_os("HOME").map(PathBuf::from);
  for program in codex_candidates(home.as_deref()) {
    match Command::new(&program).args(["app-server", "daemon", "restart"]).output() {
      Ok(output) if output.status.success() => return Ok(()),
      Ok(output) => return Err(describe_failure(output.status.code(), &String::from_utf8_lossy(&output.stderr))),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
      Err(error) => return Err(format!("Could not run {}: {error}", program.display())),
    }
  }
  Err("Codex is not installed: the codex program was not found.".into())
}
