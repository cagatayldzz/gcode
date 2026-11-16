use crate::data_dir;
use serde_json::{json, Value};
use std::{
  fs, io,
  path::{Path, PathBuf},
};

/// Which colors the interface uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeChoice {
  /// Follow the system appearance.
  #[default]
  System,
  Light,
  Dark,
}

impl ThemeChoice {
  fn name(self) -> &'static str {
    match self {
      Self::System => "system",
      Self::Light => "light",
      Self::Dark => "dark",
    }
  }

  fn from_name(name: &str) -> Self {
    match name {
      "light" => Self::Light,
      "dark" => Self::Dark,
      _ => Self::System,
    }
  }
}

/// The user's settings (`preferences.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
  pub theme: ThemeChoice,
  /// The folder new and cloned projects go into; `None` is the folder of the open project, else home.
  pub projects_dir: Option<PathBuf>,
}

impl Preferences {
  pub fn default_path() -> PathBuf {
    data_dir().join("preferences.json")
  }

  /// Reads the settings; a missing or damaged file gives the defaults.
  pub fn load(path: &Path) -> Self {
    let Some(Value::Object(root)) = fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()) else {
      return Self::default();
    };
    Self {
      theme: root.get("theme").and_then(Value::as_str).map(ThemeChoice::from_name).unwrap_or_default(),
      projects_dir: root.get("projects_dir").and_then(Value::as_str).map(PathBuf::from),
    }
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    let value = json!({
      "theme": self.theme.name(),
      "projects_dir": self.projects_dir.as_ref().map(|path| path.to_string_lossy()),
    });
    fs::write(path, serde_json::to_string_pretty(&value)?)
  }
}
