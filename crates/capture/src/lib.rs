//! Screenshots of the running app, taken with macOS's `screencapture`. The pure parts (parsing window bounds, building arguments and file
//! names) live here so they can be tested; `main.rs` runs the programs.

use std::path::{Path, PathBuf};

/// A rectangle on the screen, in points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
  pub x: i64,
  pub y: i64,
  pub width: i64,
  pub height: i64,
}

impl Bounds {
  /// The `-R` argument of `screencapture`: `x,y,width,height`.
  pub fn region(&self) -> String {
    format!("{},{},{},{}", self.x, self.y, self.width, self.height)
  }
}

/// AppleScript that prints `x, y, width, height` of the first window of the process `process`.
pub fn bounds_script(process: &str) -> String {
  let process = process.replace('\\', "\\\\").replace('"', "\\\"");
  format!(
    "tell application \"System Events\" to tell process \"{process}\"\n\
     set {{x, y}} to position of window 1\n\
     set {{w, h}} to size of window 1\n\
     return (x as text) & \", \" & (y as text) & \", \" & (w as text) & \", \" & (h as text)\n\
     end tell"
  )
}

/// Reads the output of [`bounds_script`]. `None` when it is not four whole numbers or the window has no area.
pub fn parse_bounds(output: &str) -> Option<Bounds> {
  let numbers: Vec<i64> = output.trim().split(',').map(|part| part.trim().parse().ok()).collect::<Option<_>>()?;
  match numbers[..] {
    [x, y, width, height] if width > 0 && height > 0 => Some(Bounds { x, y, width, height }),
    _ => None,
  }
}

/// The arguments of `screencapture` for a silent PNG of `region` (the whole screen when `None`) saved at `out`.
pub fn screencapture_args(region: Option<Bounds>, out: &Path) -> Vec<String> {
  let mut args = vec!["-x".to_string(), "-t".to_string(), "png".to_string()];
  if let Some(region) = region {
    args.push("-R".into());
    args.push(region.region());
  }
  args.push(out.to_string_lossy().into_owned());
  args
}

/// Where a capture goes when `--out` is not given: `<data dir>/captures/<kind>-<seconds since the epoch>.png`. The data directory follows
/// `<prefix>_DATA_DIR`, so a scratch directory keeps screenshots out of the user's real data.
pub fn default_output(kind: &str, epoch_seconds: u64) -> PathBuf {
  ws_storage::data_dir().join("captures").join(format!("{kind}-{epoch_seconds}.png"))
}

/// The usage text.
pub fn usage() -> String {
  let name = "ws-capture";
  format!(
    "Usage: {name} <window|screen> [--out FILE] [--delay SECONDS]\n\n\
     window   the app's front window\n\
     screen   the whole main screen\n\n\
     --out    where to save the PNG (default: <data dir>/captures/<kind>-<time>.png)\n\
     --delay  wait this long first, to open a menu or move the mouse\n\n\
     The terminal that runs this needs Accessibility (window) and Screen Recording permissions."
  )
}

/// What to capture and how.
#[derive(Debug, PartialEq, Eq)]
pub struct Request {
  pub window: bool,
  pub out: Option<PathBuf>,
  pub delay: u64,
}

/// Reads the command line without the program name.
pub fn parse_args(args: &[String]) -> Result<Request, String> {
  let mut args = args.iter();
  let window = match args.next().map(String::as_str) {
    Some("window") => true,
    Some("screen") => false,
    Some(other) => return Err(format!("Unknown command `{other}`")),
    None => return Err("Missing command".into()),
  };
  let (mut out, mut delay) = (None, 0);
  while let Some(flag) = args.next() {
    let value = args.next().ok_or_else(|| format!("`{flag}` needs a value"))?;
    match flag.as_str() {
      "--out" | "-o" => out = Some(PathBuf::from(value)),
      "--delay" | "-d" => delay = value.parse().map_err(|_| format!("`{value}` is not a number of seconds"))?,
      _ => return Err(format!("Unknown option `{flag}`")),
    }
  }
  Ok(Request { window, out, delay })
}
