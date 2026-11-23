use std::{
  fs,
  process::{Command, Stdio},
  thread,
  time::{Duration, SystemTime, UNIX_EPOCH},
};
use ws_capture::{bounds_script, default_output, parse_args, parse_bounds, screencapture_args, usage, Bounds};

fn main() {
  let args: Vec<String> = std::env::args().skip(1).collect();
  if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
    println!("{}", usage());
    return;
  }
  if let Err(message) = run(&args) {
    eprintln!("{message}");
    std::process::exit(1);
  }
}

fn run(args: &[String]) -> Result<(), String> {
  let request = parse_args(args).map_err(|error| format!("{error}\n\n{}", usage()))?;
  if request.delay > 0 {
    thread::sleep(Duration::from_secs(request.delay));
  }
  let region = if request.window { Some(window_bounds()?) } else { None };
  let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs());
  let out = request.out.unwrap_or_else(|| default_output(if request.window { "window" } else { "screen" }, seconds));
  if let Some(parent) = out.parent() {
    fs::create_dir_all(parent).map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
  }
  let status =
    Command::new("screencapture").args(screencapture_args(region, &out)).status().map_err(|error| format!("Could not run screencapture: {error}"))?;
  if !status.success() || !out.exists() {
    return Err("screencapture failed. Check the Screen Recording permission of this terminal.".into());
  }
  println!("{}", out.display());
  Ok(())
}

/// The front window of the running app, found through System Events.
fn window_bounds() -> Result<Bounds, String> {
  let name = ws_product::NAME;
  let output = Command::new("osascript")
    .args(["-e", &bounds_script(name)])
    .stdin(Stdio::null())
    .output()
    .map_err(|error| format!("Could not run osascript: {error}"))?;
  if !output.status.success() {
    let detail = String::from_utf8_lossy(&output.stderr);
    return Err(format!("Could not find a {name} window (is the app running, and does this terminal have Accessibility access?): {}", detail.trim()));
  }
  parse_bounds(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| format!("{name} has no window to capture."))
}
