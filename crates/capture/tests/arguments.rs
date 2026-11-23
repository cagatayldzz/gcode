use std::path::{Path, PathBuf};
use ws_capture::{bounds_script, parse_args, parse_bounds, screencapture_args, Bounds, Request};

fn args(list: &[&str]) -> Vec<String> {
  list.iter().map(|item| item.to_string()).collect()
}

#[test]
fn window_bounds_are_read_from_the_script_output() {
  assert_eq!(parse_bounds("12, 34, 1200, 800\n"), Some(Bounds { x: 12, y: 34, width: 1200, height: 800 }));
  assert_eq!(parse_bounds("-1440, 0, 900, 700"), Some(Bounds { x: -1440, y: 0, width: 900, height: 700 }));
}

#[test]
fn bad_or_empty_bounds_are_rejected() {
  assert_eq!(parse_bounds(""), None);
  assert_eq!(parse_bounds("1, 2, 3"), None);
  assert_eq!(parse_bounds("1, 2, three, 4"), None);
  assert_eq!(parse_bounds("1, 2, 0, 4"), None);
}

#[test]
fn the_script_quotes_the_process_name() {
  assert!(bounds_script("My \"App\"").contains("process \"My \\\"App\\\"\""));
}

#[test]
fn a_window_capture_passes_its_region() {
  let region = Bounds { x: 1, y: 2, width: 3, height: 4 };
  assert_eq!(screencapture_args(Some(region), Path::new("a.png")), ["-x", "-t", "png", "-R", "1,2,3,4", "a.png"]);
  assert_eq!(screencapture_args(None, Path::new("a.png")), ["-x", "-t", "png", "a.png"]);
}

#[test]
fn the_command_line_is_parsed() {
  assert_eq!(parse_args(&args(&["window"])), Ok(Request { window: true, out: None, delay: 0 }));
  assert_eq!(
    parse_args(&args(&["screen", "--out", "x.png", "--delay", "3"])),
    Ok(Request { window: false, out: Some(PathBuf::from("x.png")), delay: 3 })
  );
}

#[test]
fn a_wrong_command_line_says_what_is_wrong() {
  assert!(parse_args(&args(&[])).unwrap_err().contains("Missing"));
  assert!(parse_args(&args(&["tab"])).unwrap_err().contains("Unknown command"));
  assert!(parse_args(&args(&["window", "--out"])).unwrap_err().contains("needs a value"));
  assert!(parse_args(&args(&["window", "--delay", "soon"])).unwrap_err().contains("not a number"));
  assert!(parse_args(&args(&["window", "--size", "3"])).unwrap_err().contains("Unknown option"));
}
