//! A log for finding what is wrong: Git commands, requests to services and the steps behind the interface, each with how long it took
//! and how it ended, plus every error shown to the user. A line is `ERROR` when the step failed, `SLOW` when it took longer than
//! [`SLOW_MS`], `INFO` otherwise.
//!
//! The newest [`CAPACITY`] lines are always kept in memory (the log window of the debug button reads them with [`entries`]); a file
//! can be written as well while [`start`] is in effect. Never contains keys, tokens or request bodies. Never depends on the interface.

use std::{
  collections::VecDeque,
  fs::{File, OpenOptions},
  io::{self, Write as _},
  path::{Path, PathBuf},
  sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
  },
  time::{Instant, SystemTime, UNIX_EPOCH},
};

/// How many lines are kept in memory; older ones are dropped.
pub const CAPACITY: usize = 2000;

/// How serious a line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
  Info,
  Slow,
  Error,
}

impl Level {
  pub fn label(self) -> &'static str {
    match self {
      Self::Info => "INFO",
      Self::Slow => "SLOW",
      Self::Error => "ERROR",
    }
  }
}

/// One line of the log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
  /// Grows with every line, so a reader can tell which ones it has seen.
  pub id: u64,
  /// `HH:MM:SS.mmm` in UTC.
  pub time: String,
  pub level: Level,
  pub category: String,
  pub message: String,
}

impl Entry {
  /// The line as it is written to a file.
  pub fn text(&self) -> String {
    format!("{} {:<5} [{}] {}", self.time, self.level.label(), self.category, self.message)
  }
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static SINK: Mutex<Option<(File, PathBuf)>> = Mutex::new(None);
static BUFFER: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// The number of lines written so far (not cleared by [`clear`]); changes whenever a line is added, so a reader polls this cheaply.
pub fn version() -> u64 {
  NEXT_ID.load(Ordering::Relaxed)
}

/// The lines kept in memory, oldest first.
pub fn entries() -> Vec<Entry> {
  BUFFER.lock().map(|buffer| buffer.iter().cloned().collect()).unwrap_or_default()
}

/// Forgets the lines kept in memory.
pub fn clear() {
  if let Ok(mut buffer) = BUFFER.lock() {
    buffer.clear();
  }
  NEXT_ID.fetch_add(1, Ordering::Relaxed);
}

/// Whether lines are being written to a file.
pub fn is_active() -> bool {
  ACTIVE.load(Ordering::Relaxed)
}

/// The file being written, while recording.
pub fn current_path() -> Option<PathBuf> {
  SINK.lock().ok()?.as_ref().map(|(_, path)| path.clone())
}

/// Starts writing the log to `path` (appended to, created with its folder when needed).
pub fn start(path: &Path) -> io::Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let file = OpenOptions::new().create(true).append(true).open(path)?;
  *SINK.lock().map_err(|_| io::Error::other("the activity log is unavailable"))? = Some((file, path.to_path_buf()));
  ACTIVE.store(true, Ordering::Relaxed);
  write_line(Level::Info, "log", "recording started");
  Ok(())
}

/// Stops writing the log to a file; the lines in memory keep coming.
pub fn stop() {
  if is_active() {
    write_line(Level::Info, "log", "recording stopped");
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

/// A step that takes longer than this (in milliseconds) is marked `SLOW`.
pub const SLOW_MS: u128 = 500;

fn write_line(level: Level, category: &str, message: &str) {
  let entry =
    Entry { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), time: clock(), level, category: category.to_string(), message: message.to_string() };
  if is_active() {
    if let Ok(mut sink) = SINK.lock() {
      if let Some((file, _)) = sink.as_mut() {
        let _ = writeln!(file, "{}", entry.text());
      }
    }
  }
  if let Ok(mut buffer) = BUFFER.lock() {
    if buffer.len() >= CAPACITY {
      buffer.pop_front();
    }
    buffer.push_back(entry);
  }
}

/// Records one line; `message` is built here, so keep it cheap.
pub fn record(category: &str, message: impl FnOnce() -> String) {
  write_line(Level::Info, category, &message());
}

/// Records something that went wrong, as an `ERROR` line.
pub fn error(category: &str, message: impl FnOnce() -> String) {
  write_line(Level::Error, category, &message());
}

/// Runs `work` and writes one line: what it was, how it ended (`outcome` describes the result; one that starts with `failed` or
/// `not run` marks the line `ERROR`) and how long it took.
pub fn timed<T>(category: &str, label: impl FnOnce() -> String, work: impl FnOnce() -> T, outcome: impl FnOnce(&T) -> String) -> T {
  let label = label();
  let started = Instant::now();
  let result = work();
  let (outcome, elapsed) = (outcome(&result), started.elapsed().as_millis());
  let failed = outcome.starts_with("failed") || outcome.starts_with("not run");
  let level = if failed {
    Level::Error
  } else if elapsed > SLOW_MS {
    Level::Slow
  } else {
    Level::Info
  };
  write_line(level, category, &format!("{label} → {outcome} ({elapsed} ms)"));
  result
}

/// Like [`timed`], for work that repeats on a timer: the line is written only when the work failed or took at least `notable_ms`,
/// so that routine rounds do not fill the log.
pub fn timed_quiet<T>(
  category: &str,
  notable_ms: u128,
  label: impl FnOnce() -> String,
  work: impl FnOnce() -> T,
  outcome: impl FnOnce(&T) -> String,
) -> T {
  let started = Instant::now();
  let result = work();
  let elapsed = started.elapsed().as_millis();
  let outcome = outcome(&result);
  let failed = outcome.starts_with("failed") || outcome.starts_with("not run");
  if failed || elapsed >= notable_ms {
    let level = if failed {
      Level::Error
    } else if elapsed > SLOW_MS {
      Level::Slow
    } else {
      Level::Info
    };
    write_line(level, category, &format!("{} → {outcome} ({elapsed} ms)", label()));
  }
  result
}
