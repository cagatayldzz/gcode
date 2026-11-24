//! A log of what the app does, switched on by the user (the debug button in the title bar): Git commands and requests to services,
//! each with how long it took and how it ended. While it is off, recording costs one atomic read. Never contains keys, tokens or
//! request bodies. Never depends on the interface.

use std::{
  fs::{File, OpenOptions},
  io::{self, Write as _},
  path::{Path, PathBuf},
  sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
  },
  time::{Instant, SystemTime, UNIX_EPOCH},
};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static SINK: Mutex<Option<(File, PathBuf)>> = Mutex::new(None);

/// Whether activity is being recorded.
pub fn is_active() -> bool {
  ACTIVE.load(Ordering::Relaxed)
}

/// The file being written, while recording.
pub fn current_path() -> Option<PathBuf> {
  SINK.lock().ok()?.as_ref().map(|(_, path)| path.clone())
}

/// Starts recording into `path` (appended to, created with its folder when needed).
pub fn start(path: &Path) -> io::Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let file = OpenOptions::new().create(true).append(true).open(path)?;
  *SINK.lock().map_err(|_| io::Error::other("the activity log is unavailable"))? = Some((file, path.to_path_buf()));
  ACTIVE.store(true, Ordering::Relaxed);
  write_line("log", "recording started");
  Ok(())
}

/// Stops recording.
pub fn stop() {
  if is_active() {
    write_line("log", "recording stopped");
  }
  ACTIVE.store(false, Ordering::Relaxed);
  if let Ok(mut sink) = SINK.lock() {
    *sink = None;
  }
}

/// `HH:MM:SS.mmm` in UTC.
fn clock() -> String {
  let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
  let seconds = elapsed.as_secs() % 86_400;
  format!("{:02}:{:02}:{:02}.{:03}", seconds / 3600, seconds / 60 % 60, seconds % 60, elapsed.subsec_millis())
}

fn write_line(category: &str, message: &str) {
  let Ok(mut sink) = SINK.lock() else { return };
  if let Some((file, _)) = sink.as_mut() {
    let _ = writeln!(file, "{} [{category}] {message}", clock());
  }
}

/// Records one line when recording; `message` is only built then.
pub fn record(category: &str, message: impl FnOnce() -> String) {
  if is_active() {
    write_line(category, &message());
  }
}

/// Runs `work` and, when recording, writes one line: what it was, how it ended (`outcome` describes the result) and how long it took.
pub fn timed<T>(category: &str, label: impl FnOnce() -> String, work: impl FnOnce() -> T, outcome: impl FnOnce(&T) -> String) -> T {
  if !is_active() {
    return work();
  }
  let label = label();
  let started = Instant::now();
  let result = work();
  write_line(category, &format!("{label} → {} ({} ms)", outcome(&result), started.elapsed().as_millis()));
  result
}
