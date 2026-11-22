use serde_json::{json, Value};
use std::{
  io,
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

/// The smallest and largest configurable font sizes, in pixels.
pub const FONT_SIZES: std::ops::RangeInclusive<u32> = 9..=24;

/// The size of code when the user has not chosen one.
pub const DEFAULT_CODE_FONT_SIZE: u32 = 13;

/// The default font size of file and folder names in Explorer.
pub const DEFAULT_EXPLORER_FONT_SIZE: u32 = 14;

/// How the activity bar (the column of buttons at the left edge) is arranged: the order of its buttons and the ones the user
/// hid. Buttons are named by id; ids the bar does not know are ignored and new ones come after the saved ones, so a version that
/// adds a button does not need to migrate anything.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActivityBarLayout {
  pub order: Vec<String>,
  pub hidden: Vec<String>,
}

impl ActivityBarLayout {
  /// Every button of `all`, in the saved order (hidden ones included); buttons not saved yet follow in their default order.
  pub fn arranged<'a>(&self, all: &[&'a str]) -> Vec<&'a str> {
    let mut arranged: Vec<&'a str> = self.order.iter().filter_map(|id| all.iter().copied().find(|known| known == id)).collect();
    arranged.dedup_by(|a, b| a == b);
    for id in all {
      if !arranged.contains(id) {
        arranged.push(id);
      }
    }
    arranged
  }

  /// The buttons to show, in order.
  pub fn visible<'a>(&self, all: &[&'a str]) -> Vec<&'a str> {
    self.arranged(all).into_iter().filter(|id| !self.is_hidden(id)).collect()
  }

  pub fn is_hidden(&self, id: &str) -> bool {
    self.hidden.iter().any(|hidden| hidden == id)
  }

  pub fn set_hidden(&mut self, id: &str, hidden: bool) {
    self.hidden.retain(|other| other != id);
    if hidden {
      self.hidden.push(id.to_owned());
    }
  }

  /// Moves a button up (`-1`) or down (`1`) among all of `all`, hidden ones included, so the order stays what the settings list
  /// shows.
  pub fn move_by(&mut self, all: &[&str], id: &str, delta: isize) {
    let mut arranged: Vec<String> = self.arranged(all).into_iter().map(str::to_owned).collect();
    let Some(from) = arranged.iter().position(|other| other == id) else { return };
    let to = from.saturating_add_signed(delta).min(arranged.len() - 1);
    let moved = arranged.remove(from);
    arranged.insert(to, moved);
    self.order = arranged;
  }
}

/// The user's settings (`preferences.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
  pub theme: ThemeChoice,
  /// The folder new and cloned projects go into; `None` is the folder of the open project, else home.
  pub projects_dir: Option<PathBuf>,
  /// The size of the code in the editor and in changes, in pixels, within [`FONT_SIZES`].
  pub code_font_size: u32,
  /// The size of file and folder names in Explorer, in pixels.
  pub explorer_font_size: u32,
  /// The instructions the commit message generator follows besides its rules; `None` is the built-in text, `Some("")` none at all.
  pub commit_instructions: Option<String>,
  /// Ask GitHub for a newer Vekro release when the app starts.
  pub check_updates: bool,
  /// The order of the activity bar's buttons and which of them are hidden.
  pub activity_bar: ActivityBarLayout,
}

impl Default for Preferences {
  fn default() -> Self {
    Self {
      theme: ThemeChoice::default(),
      projects_dir: None,
      code_font_size: DEFAULT_CODE_FONT_SIZE,
      explorer_font_size: DEFAULT_EXPLORER_FONT_SIZE,
      commit_instructions: None,
      check_updates: true,
      activity_bar: ActivityBarLayout::default(),
    }
  }
}

impl Preferences {
  /// Reads the settings; a missing or damaged file gives the defaults.
  pub fn load(path: &Path) -> Self {
    let Some(Value::Object(root)) = crate::read_json(path) else {
      return Self::default();
    };
    Self {
      theme: root.get("theme").and_then(Value::as_str).map(ThemeChoice::from_name).unwrap_or_default(),
      projects_dir: root.get("projects_dir").and_then(Value::as_str).map(PathBuf::from),
      code_font_size: font_size(root.get("code_font_size"), DEFAULT_CODE_FONT_SIZE),
      explorer_font_size: font_size(root.get("explorer_font_size"), DEFAULT_EXPLORER_FONT_SIZE),
      commit_instructions: root.get("commit_instructions").and_then(Value::as_str).map(str::to_owned),
      check_updates: root.get("check_updates").and_then(Value::as_bool).unwrap_or(true),
      activity_bar: ActivityBarLayout { order: strings(root.get("activity_order")), hidden: strings(root.get("activity_hidden")) },
    }
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    let value = json!({
      "theme": self.theme.name(),
      "projects_dir": self.projects_dir.as_ref().map(|path| path.to_string_lossy()),
      "code_font_size": self.code_font_size,
      "explorer_font_size": self.explorer_font_size,
      "commit_instructions": self.commit_instructions,
      "check_updates": self.check_updates,
      "activity_order": self.activity_bar.order,
      "activity_hidden": self.activity_bar.hidden,
    });
    crate::write_json(path, &value)
  }
}

fn font_size(value: Option<&Value>, default: u32) -> u32 {
  value.and_then(Value::as_u64).map_or(default, |size| size.clamp(*FONT_SIZES.start() as u64, *FONT_SIZES.end() as u64) as u32)
}

/// The strings of a JSON array; anything else gives none.
fn strings(value: Option<&Value>) -> Vec<String> {
  value.and_then(Value::as_array).map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default()
}
