use std::{fs, sync::Mutex};

/// The log is shared by the whole process, so the tests take turns.
static TURN: Mutex<()> = Mutex::new(());

#[test]
fn work_is_recorded_only_while_recording_is_on() {
  let _turn = TURN.lock().unwrap_or_else(|error| error.into_inner());
  let dir = std::env::temp_dir().join(format!("ws-activity-{}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  let path = dir.join("nested").join("activity.log");

  assert_eq!(ws_activity::timed("git", || "before".into(), || 1, |_| "ok".into()), 1);
  assert!(!path.exists());

  ws_activity::start(&path).unwrap();
  assert!(ws_activity::is_active());
  assert_eq!(ws_activity::current_path().as_deref(), Some(path.as_path()));
  let answer = ws_activity::timed("git", || "git status".into(), || 7, |value| format!("ok {value}"));
  ws_activity::record("http", || "GET https://example.com".into());
  ws_activity::timed("git", || "git push".into(), || (), |_| "failed: no remote".into());
  ws_activity::error("ui", || "Could not save".into());
  ws_activity::stop();
  ws_activity::record("git", || "after".into());
  assert_eq!(answer, 7);
  assert!(!ws_activity::is_active());

  let text = fs::read_to_string(&path).unwrap();
  assert!(text.contains("INFO  [git] git status → ok 7 ("));
  assert!(text.contains("[http] GET https://example.com"));
  assert!(text.contains("ERROR [git] git push → failed: no remote ("));
  assert!(text.contains("ERROR [ui] Could not save"));
  assert!(text.contains("recording started") && text.contains("recording stopped"));
  assert!(!text.contains("before") && !text.contains("after"));
  let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_newest_lines_are_kept_in_memory_and_can_be_cleared() {
  let _turn = TURN.lock().unwrap_or_else(|error| error.into_inner());
  ws_activity::clear();
  let before = ws_activity::version();
  ws_activity::record("http", || "GET https://example.com/memory".into());
  ws_activity::timed("git", || "git memory".into(), || (), |_| "failed: nope".into());
  assert!(ws_activity::version() >= before + 2);

  let entries = ws_activity::entries();
  let line = entries.iter().find(|entry| entry.message.contains("example.com/memory")).unwrap();
  assert_eq!(line.level, ws_activity::Level::Info);
  let failed = entries.iter().find(|entry| entry.message.starts_with("git memory")).unwrap();
  assert_eq!(failed.level, ws_activity::Level::Error);
  assert!(failed.id > line.id);
  assert!(failed.text().contains("ERROR [git] git memory → failed: nope ("));

  for index in 0..ws_activity::CAPACITY + 10 {
    ws_activity::record("flood", || index.to_string());
  }
  assert_eq!(ws_activity::entries().len(), ws_activity::CAPACITY);

  ws_activity::clear();
  assert!(ws_activity::entries().is_empty());
}
