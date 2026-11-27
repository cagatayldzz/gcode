use crate::data_dir;
use serde_json::{json, Value};
use std::{
  fs, io,
  path::{Path, PathBuf},
};

/// The id of the profile that exists from the start. Its data stays directly in the data directory, where it was before profiles
/// existed, so that nothing has to be moved.
pub const DEFAULT_PROFILE: &str = "default";

/// A separate set of projects, settings and connected accounts, like a browser profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
  /// Stable; names the profile's folder. Never changes when the profile is renamed.
  pub id: String,
  pub name: String,
}

impl Profile {
  /// The profile every installation has.
  pub fn default_profile() -> Self {
    Self { id: DEFAULT_PROFILE.to_string(), name: "Default".to_string() }
  }

  pub fn is_default(&self) -> bool {
    self.id == DEFAULT_PROFILE
  }

  /// The folder of the profile's files: the data directory itself for the default profile, else `profiles/<id>`.
  pub fn dir(&self) -> PathBuf {
    if self.is_default() {
      data_dir()
    } else {
      data_dir().join("profiles").join(&self.id)
    }
  }

  /// Checkouts are kept outside the profile directory so removing a profile cannot delete working files.
  pub fn agent_worktrees_path(&self) -> PathBuf {
    data_dir().join("agent-worktrees").join(&self.id)
  }

  pub fn projects_path(&self) -> PathBuf {
    self.dir().join("projects.json")
  }

  pub fn preferences_path(&self) -> PathBuf {
    self.dir().join("preferences.json")
  }

  pub fn accounts_path(&self) -> PathBuf {
    self.dir().join("accounts.json")
  }

  /// The prefix of this profile's entries in the Keychain, so that the same account connected in two profiles keeps two tokens.
  /// `None` for the default profile, whose entries are the ones that existed before profiles.
  pub fn keychain_namespace(&self) -> Option<String> {
    (!self.is_default()).then(|| self.id.clone())
  }
}

/// The profiles and the one that was used last (`profiles.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileList {
  pub profiles: Vec<Profile>,
  /// The id of the profile a new window opens with when no window says otherwise.
  pub last: String,
}

impl Default for ProfileList {
  fn default() -> Self {
    Self { profiles: vec![Profile::default_profile()], last: DEFAULT_PROFILE.to_string() }
  }
}

impl ProfileList {
  pub fn default_path() -> PathBuf {
    data_dir().join("profiles.json")
  }

  /// Reads the list; a missing or damaged file gives the default profile alone. The default profile is always in the list.
  pub fn load(path: &Path) -> Self {
    let Some(Value::Object(root)) = crate::read_json(path) else {
      return Self::default();
    };
    let mut profiles: Vec<Profile> = Vec::new();
    for item in root.get("profiles").and_then(Value::as_array).into_iter().flatten() {
      let text = |key: &str| item.get(key).and_then(Value::as_str).map(str::to_owned);
      let Some(id) = text("id").filter(|id| valid_id(id)) else { continue };
      if profiles.iter().any(|profile| profile.id == id) {
        continue;
      }
      let name = text("name").filter(|name| !name.trim().is_empty()).unwrap_or_else(|| id.clone());
      profiles.push(Profile { id, name });
    }
    if !profiles.iter().any(Profile::is_default) {
      profiles.insert(0, Profile::default_profile());
    }
    let last = root.get("last").and_then(Value::as_str).filter(|last| profiles.iter().any(|profile| profile.id == *last)).unwrap_or(DEFAULT_PROFILE);
    Self { last: last.to_string(), profiles }
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    let value = json!({
      "profiles": self.profiles.iter().map(|profile| json!({ "id": profile.id, "name": profile.name })).collect::<Vec<_>>(),
      "last": self.last,
    });
    crate::write_json(path, &value)
  }

  pub fn get(&self, id: &str) -> Option<&Profile> {
    self.profiles.iter().find(|profile| profile.id == id)
  }

  /// The profile a window opens with by default: the one used last.
  pub fn last_used(&self) -> Profile {
    self.get(&self.last).cloned().unwrap_or_else(Profile::default_profile)
  }

  /// Remembers the profile a window was last used with.
  pub fn set_last(&mut self, id: &str) {
    if self.get(id).is_some() {
      self.last = id.to_string();
    }
  }

  /// Adds a profile with this name; its id comes from the name and is made unique. A blank name gives `None`.
  pub fn add(&mut self, name: &str) -> Option<Profile> {
    let name = name.trim();
    if name.is_empty() {
      return None;
    }
    let base: String = name
      .chars()
      .map(|character| if character.is_ascii_alphanumeric() { character.to_ascii_lowercase() } else { '-' })
      .collect::<String>()
      .split('-')
      .filter(|part| !part.is_empty())
      .collect::<Vec<_>>()
      .join("-");
    let base = if base.is_empty() { "profile".to_string() } else { base };
    let mut id = base.clone();
    let mut number = 2;
    while self.get(&id).is_some() {
      id = format!("{base}-{number}");
      number += 1;
    }
    let profile = Profile { id, name: name.to_string() };
    self.profiles.push(profile.clone());
    Some(profile)
  }

  /// Renames a profile; a blank name changes nothing.
  pub fn rename(&mut self, id: &str, name: &str) {
    let name = name.trim();
    if name.is_empty() {
      return;
    }
    if let Some(profile) = self.profiles.iter_mut().find(|profile| profile.id == id) {
      profile.name = name.to_string();
    }
  }

  /// Removes a profile from the list; the default profile cannot be removed. Returns the removed profile, whose files the caller
  /// deletes.
  pub fn remove(&mut self, id: &str) -> Option<Profile> {
    if id == DEFAULT_PROFILE {
      return None;
    }
    let index = self.profiles.iter().position(|profile| profile.id == id)?;
    let removed = self.profiles.remove(index);
    if self.last == removed.id {
      self.last = DEFAULT_PROFILE.to_string();
    }
    Some(removed)
  }
}

/// An id becomes a folder name, so it holds no path separators or dots.
fn valid_id(id: &str) -> bool {
  !id.is_empty() && id.chars().all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
}

/// Deletes the files of a removed profile (never the default profile's, which share the data directory).
pub fn delete_profile_files(profile: &Profile) -> io::Result<()> {
  if profile.is_default() || !valid_id(&profile.id) {
    return Ok(());
  }
  match fs::remove_dir_all(profile.dir()) {
    Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
    _ => Ok(()),
  }
}
