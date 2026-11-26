use ws_storage::{format_report, write_report, KEPT_REPORTS};

#[test]
fn a_report_names_the_time_thread_place_and_message() {
  // 2026-11-26 08:47:38 UTC
  let report = format_report(1_795_682_858, "0.1.0", "main", "crates/ui/src/profiles.rs:10:5", "cannot read Workspace", "0: frame");
  assert!(report.contains("2026-11-26 08:47:38 UTC"), "{report}");
  for part in ["version:  0.1.0", "thread:   main", "location: crates/ui/src/profiles.rs:10:5", "message:  cannot read Workspace", "0: frame"] {
    assert!(report.contains(part), "{part} in {report}");
  }
}

#[test]
fn a_report_does_not_show_the_home_folder() {
  let home = std::env::var("HOME").unwrap();
  let report = format_report(0, "0.1.0", "main", &format!("{home}/code/a.rs:1:1"), "boom", "");
  assert!(!report.contains(&home) && report.contains("~/code/a.rs:1:1"), "{report}");
}

#[test]
fn reports_are_written_into_the_folder_and_only_the_newest_stay() {
  let dir = tempfile::tempdir().unwrap();
  let logs = dir.path().join("logs");
  for index in 0..(KEPT_REPORTS as u64 + 5) {
    write_report(&logs, &format!("report {index}"), 1_795_769_258 + index).unwrap();
  }
  let mut names: Vec<String> = std::fs::read_dir(&logs).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
  names.sort();
  assert_eq!(names.len(), KEPT_REPORTS);
  assert!(names.iter().all(|name| name.starts_with("crash-") && name.ends_with(".log")));
  let newest = std::fs::read_to_string(logs.join(names.last().unwrap())).unwrap();
  assert_eq!(newest, format!("report {}", KEPT_REPORTS + 4), "the oldest were removed, not the newest");
}

/// The panic hook is global, so the test runs it in a child process that has its own data folder.
#[test]
fn a_panic_is_written_to_the_logs_folder() {
  const CHILD: &str = "WS_TEST_PANIC_CHILD";
  if std::env::var_os(CHILD).is_some() {
    ws_storage::install_panic_hook();
    let _ = std::panic::catch_unwind(|| panic!("the profiles page read the workspace"));
    return;
  }
  let dir = tempfile::tempdir().unwrap();
  let status = std::process::Command::new(std::env::current_exe().unwrap())
    .args(["--exact", "a_panic_is_written_to_the_logs_folder", "--nocapture"])
    .env(CHILD, "1")
    .env(ws_product::env_name("DATA_DIR"), dir.path())
    .status()
    .unwrap();
  assert!(status.success());
  let reports: Vec<_> = std::fs::read_dir(dir.path().join("logs")).unwrap().map(|entry| entry.unwrap().path()).collect();
  assert_eq!(reports.len(), 1, "{reports:?}");
  let text = std::fs::read_to_string(&reports[0]).unwrap();
  assert!(text.contains("the profiles page read the workspace") && text.contains("crash_log.rs"), "{text}");
}
