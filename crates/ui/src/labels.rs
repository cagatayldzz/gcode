//! Short texts for numbers: how long ago, how big. No interface in here, so they are tested alone.

/// How long ago `time` was, for a label: "just now", "5m ago", "3h ago", "2d ago".
pub(crate) fn time_ago(time: std::time::SystemTime) -> String {
  let seconds = time.elapsed().map_or(0, |elapsed| elapsed.as_secs());
  match seconds {
    0..=59 => "just now".to_string(),
    60..=3599 => format!("{}m ago", seconds / 60),
    3600..=86399 => format!("{}h ago", seconds / 3600),
    _ => format!("{}d ago", seconds / 86400),
  }
}

/// A size for a label: "812 B", "3.4 KB", "12.1 MB", "1.2 GB".
pub(crate) fn size_label(bytes: u64) -> String {
  const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
  if bytes < 1024 {
    return format!("{bytes} B");
  }
  let mut value = bytes as f64 / 1024.;
  let mut unit = 0;
  while value >= 1024. && unit + 1 < UNITS.len() {
    value /= 1024.;
    unit += 1;
  }
  format!("{value:.1} {}", UNITS[unit])
}
