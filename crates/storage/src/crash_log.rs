//! Crash reports: when the app panics, what happened is written to a file in the `logs` folder of the data directory before the process
//! ends, so that a crash that leaves nothing on screen can still be looked into afterwards.

use crate::data_dir;
use std::{
  backtrace::Backtrace,
  fs, io,
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

/// How many crash reports stay; the oldest ones are removed.
pub const KEPT_REPORTS: usize = 20;

const PREFIX: &str = "crash-";
const SUFFIX: &str = ".log";

/// The folder of the logs.
pub fn logs_dir() -> PathBuf {
  data_dir().join("logs")
}

/// From now on a panic anywhere in the app is written to a file in [`logs_dir`], then reported as before (to the error output). Call it
/// first thing at startup.
pub fn install_panic_hook() {
  let previous = std::panic::take_hook();
  std::panic::set_hook(Box::new(move |info| {
    let payload = info.payload();
    let message = payload
      .downcast_ref::<&str>()
      .map(|text| (*text).to_string())
      .or_else(|| payload.downcast_ref::<String>().cloned())
      .unwrap_or_else(|| "(no message)".to_string());
    let location = info.location().map_or_else(|| "unknown".to_string(), |at| format!("{}:{}:{}", at.file(), at.line(), at.column()));
    let thread = std::thread::current();
    let seconds = now_seconds();
    let report = format_report(
      seconds,
      env!("CARGO_PKG_VERSION"),
      thread.name().unwrap_or("unnamed"),
      &location,
      &message,
      &Backtrace::force_capture().to_string(),
    );
    // The report is best effort: a failure here must not hide the panic itself.
    match write_report(&logs_dir(), &report, seconds) {
      Ok(path) => eprintln!("A crash report was written to {}", path.display()),
      Err(error) => eprintln!("Could not write the crash report: {error}"),
    }
    previous(info);
  }));
}

fn now_seconds() -> u64 {
  SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs())
}

/// The text of a report. The home folder is shown as `~`, so that a report can be shared without the user's name in it.
pub fn format_report(seconds: u64, version: &str, thread: &str, location: &str, message: &str, backtrace: &str) -> String {
  let text = format!(
    "{} crashed\n\ntime:     {} UTC\nversion:  {version}\nthread:   {thread}\nlocation: {location}\nmessage:  {message}\n\nbacktrace:\n{backtrace}\n",
    ws_product::NAME,
    stamp(seconds, true),
  );
  match std::env::var("HOME") {
    Ok(home) if home.len() > 1 => text.replace(&home, "~"),
    _ => text,
  }
}

/// Writes a report as `crash-<date>-<time>-<pid>.log` in `dir` and removes the oldest ones beyond [`KEPT_REPORTS`]. Returns the file.
pub fn write_report(dir: &Path, report: &str, seconds: u64) -> io::Result<PathBuf> {
  fs::create_dir_all(dir)?;
  let path = dir.join(format!("{PREFIX}{}-{}{SUFFIX}", stamp(seconds, false), std::process::id()));
  fs::write(&path, report)?;
  prune(dir)?;
  Ok(path)
}

/// Keeps the newest [`KEPT_REPORTS`] reports (their names sort by time).
fn prune(dir: &Path) -> io::Result<()> {
  let mut reports: Vec<PathBuf> = fs::read_dir(dir)?
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|path| path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with(PREFIX) && name.ends_with(SUFFIX)))
    .collect();
  reports.sort();
  let extra = reports.len().saturating_sub(KEPT_REPORTS);
  for old in reports.into_iter().take(extra) {
    let _ = fs::remove_file(old);
  }
  Ok(())
}

/// `2026-11-26 11:47:38` (readable) or `20261126-114738` (for a file name), in UTC.
fn stamp(seconds: u64, readable: bool) -> String {
  let (days, rest) = ((seconds / 86_400) as i64, seconds % 86_400);
  let (hour, minute, second) = (rest / 3_600, rest % 3_600 / 60, rest % 60);
  // Days since 1970-01-01 to a calendar date (the civil-from-days algorithm).
  let shifted = days + 719_468;
  let era = shifted.div_euclid(146_097);
  let day_of_era = shifted.rem_euclid(146_097);
  let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
  let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
  let month_index = (5 * day_of_year + 2) / 153;
  let day = day_of_year - (153 * month_index + 2) / 5 + 1;
  let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
  let year = year_of_era + era * 400 + i64::from(month <= 2);
  if readable {
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
  } else {
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
  }
}
