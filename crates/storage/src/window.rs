use crate::data_dir;
use serde_json::{json, Value};
use std::{
  io,
  path::{Path, PathBuf},
};

/// How the window was when it was last closed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowMode {
  #[default]
  Windowed,
  Maximized,
  Fullscreen,
}

impl WindowMode {
  fn name(self) -> &'static str {
    match self {
      Self::Windowed => "windowed",
      Self::Maximized => "maximized",
      Self::Fullscreen => "fullscreen",
    }
  }

  fn from_name(name: &str) -> Self {
    match name {
      "maximized" => Self::Maximized,
      "fullscreen" => Self::Fullscreen,
      _ => Self::Windowed,
    }
  }
}

/// Where the window was and how big (`window.json`). For a maximized or full-screen window the rectangle is the one to
/// restore to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowPlacement {
  pub x: f32,
  pub y: f32,
  pub width: f32,
  pub height: f32,
  pub mode: WindowMode,
}

impl WindowPlacement {
  /// The file the placement is kept in.
  pub fn default_path() -> PathBuf {
    data_dir().join("window.json")
  }

  /// Reads the placement; a missing, damaged or unusable file (a size that is not a real size) gives `None`.
  pub fn load(path: &Path) -> Option<Self> {
    let Value::Object(root) = crate::read_json(path)? else { return None };
    let number = |key: &str| root.get(key).and_then(Value::as_f64).map(|value| value as f32).filter(|value| value.is_finite());
    let placement = Self {
      x: number("x")?,
      y: number("y")?,
      width: number("width")?,
      height: number("height")?,
      mode: root.get("mode").and_then(Value::as_str).map(WindowMode::from_name).unwrap_or_default(),
    };
    (placement.width >= 100. && placement.height >= 100.).then_some(placement)
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    let value = json!({ "x": self.x, "y": self.y, "width": self.width, "height": self.height, "mode": self.mode.name() });
    crate::write_json(path, &value)
  }
}
