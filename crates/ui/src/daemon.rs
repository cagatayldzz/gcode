//! Restarting the Codex app-server daemon: it can stay up for days and stop answering, and Codex then shows "Reconnecting to server…" or sits at
//! "Waiting for startup".

use std::{
  io::Read,
  path::{Path, PathBuf},
  process::{Command, Stdio},
  time::{Duration, Instant},
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

/// How long `daemon restart` may run before it counts as stuck. A healthy restart takes a few seconds.
const RESTART_TIMEOUT: Duration = Duration::from_secs(20);

/// Matches the processes Codex manages for its daemon (the daemon, its code-mode host and any restart command stuck on it) and nothing else,
/// so Codex sessions running in terminals are left alone.
pub(crate) const DAEMON_PROCESS_PATTERN: &str = "app-server-daemon/.*(codex app-server|codex-code-mode-host)";

/// How a command ended.
pub(crate) enum Outcome {
  Finished { code: Option<i32>, stderr: String },
  TimedOut,
}

/// Runs `program` and kills it when it has not ended after `timeout`. Blocks, so call it off the interface thread.
pub(crate) fn run_with_timeout(program: &Path, args: &[&str], timeout: Duration) -> std::io::Result<Outcome> {
  let mut child = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn()?;
  let mut stderr = child.stderr.take();
  let reader = std::thread::spawn(move || {
    let mut text = String::new();
    if let Some(stderr) = stderr.as_mut() {
      let _ = stderr.read_to_string(&mut text);
    }
    text
  });
  let deadline = Instant::now() + timeout;
  loop {
    if let Some(status) = child.try_wait()? {
      return Ok(Outcome::Finished { code: status.code(), stderr: reader.join().unwrap_or_default() });
    }
    if Instant::now() >= deadline {
      let _ = child.kill();
      let _ = child.wait();
      return Ok(Outcome::TimedOut);
    }
    std::thread::sleep(Duration::from_millis(50));
  }
}

/// Stops the daemon's processes and removes the control socket they left behind, so the next start is clean.
fn clear_stuck_daemon(home: Option<&Path>) {
  let _ = Command::new("pkill").args(["-f", DAEMON_PROCESS_PATTERN]).stdout(Stdio::null()).stderr(Stdio::null()).status();
  if let Some(home) = home {
    let _ = std::fs::remove_file(home.join(".codex/app-server-control/app-server-control.sock"));
  }
}

fn restart_once(program: &Path) -> std::io::Result<Outcome> {
  run_with_timeout(program, &["app-server", "daemon", "restart"], RESTART_TIMEOUT)
}

/// Runs `codex app-server daemon restart`. When that hangs or fails, stops the daemon's processes and tries once more. Blocks, so call it
/// off the interface thread.
pub(crate) fn restart_codex_daemon() -> Result<(), String> {
  let home = std::env::var_os("HOME").map(PathBuf::from);
  for program in codex_candidates(home.as_deref()) {
    let first = match restart_once(&program) {
      Ok(outcome) => outcome,
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
      Err(error) => return Err(format!("Could not run {}: {error}", program.display())),
    };
    if matches!(first, Outcome::Finished { code: Some(0), .. }) {
      return Ok(());
    }
    clear_stuck_daemon(home.as_deref());
    return match restart_once(&program) {
      Ok(Outcome::Finished { code: Some(0), .. }) => Ok(()),
      Ok(Outcome::Finished { code, stderr }) => Err(describe_failure(code, &stderr)),
      Ok(Outcome::TimedOut) => Err("Could not restart the Codex daemon: it did not answer, even after stopping it.".into()),
      Err(error) => Err(format!("Could not run {}: {error}", program.display())),
    };
  }
  Err("Codex is not installed: the codex program was not found.".into())
}
