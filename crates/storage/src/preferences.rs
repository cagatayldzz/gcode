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

/// The smallest and the largest size of code, in pixels.
pub const CODE_FONT_SIZES: std::ops::RangeInclusive<u32> = 9..=24;

/// The size of code when the user has not chosen one.
pub const DEFAULT_CODE_FONT_SIZE: u32 = 13;

/// The user's settings (`preferences.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
  pub theme: ThemeChoice,
  /// The folder new and cloned projects go into; `None` is the folder of the open project, else home.
  pub projects_dir: Option<PathBuf>,
  /// The size of the code in the editor and in changes, in pixels, within [`CODE_FONT_SIZES`].
  pub code_font_size: u32,
  /// The instructions the commit message generator follows besides its rules; `None` is the built-in text, `Some("")` none at all.
  pub commit_instructions: Option<String>,
  /// Ask GitHub for a newer Vekro release when the app starts.
  pub check_updates: bool,
}

impl Default for Preferences {
  fn default() -> Self {
    Self { theme: ThemeChoice::default(), projects_dir: None, code_font_size: DEFAULT_CODE_FONT_SIZE, commit_instructions: None, check_updates: true }
  }
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
      code_font_size: root
        .get("code_font_size")
        .and_then(Value::as_u64)
        .map_or(DEFAULT_CODE_FONT_SIZE, |size| size.clamp(*CODE_FONT_SIZES.start() as u64, *CODE_FONT_SIZES.end() as u64) as u32),
      commit_instructions: root.get("commit_instructions").and_then(Value::as_str).map(str::to_owned),
      check_updates: root.get("check_updates").and_then(Value::as_bool).unwrap_or(true),
    }
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    let value = json!({
      "theme": self.theme.name(),
      "projects_dir": self.projects_dir.as_ref().map(|path| path.to_string_lossy()),
      "code_font_size": self.code_font_size,
      "commit_instructions": self.commit_instructions,
      "check_updates": self.check_updates,
    });
    fs::write(path, serde_json::to_string_pretty(&value)?)
  }
}
