use crate::data_dir;
use serde_json::{json, Value};
use std::{
  fs, io,
  path::{Path, PathBuf},
};

/// A folder the user works in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
  pub path: PathBuf,
}

impl Project {
  /// The folder's name, shown in the project list.
  pub fn name(&self) -> String {
    self.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| self.path.display().to_string())
  }
}

/// The projects in the sidebar and the one that was open last (`projects.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectList {
  pub projects: Vec<Project>,
  pub active: Option<PathBuf>,
}

impl ProjectList {
  pub fn default_path() -> PathBuf {
    data_dir().join("projects.json")
  }

  /// Reads the list; a missing or damaged file gives an empty list.
  pub fn load(path: &Path) -> Self {
    let Some(Value::Object(root)) = fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()) else {
      return Self::default();
    };
    let projects = root
      .get("projects")
      .and_then(Value::as_array)
      .map(|paths| paths.iter().filter_map(Value::as_str).map(|path| Project { path: PathBuf::from(path) }).collect())
      .unwrap_or_default();
    let active = root.get("active").and_then(Value::as_str).map(PathBuf::from);
    Self { projects, active }
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    let value = json!({
      "projects": self.projects.iter().map(|project| project.path.to_string_lossy()).collect::<Vec<_>>(),
      "active": self.active.as_ref().map(|path| path.to_string_lossy()),
    });
    fs::write(path, serde_json::to_string_pretty(&value)?)
  }

  /// Adds a folder (once) and makes it the active project.
  pub fn add(&mut self, path: PathBuf) {
    if !self.projects.iter().any(|project| project.path == path) {
      self.projects.push(Project { path: path.clone() });
    }
    self.active = Some(path);
  }

  /// Removes a project from the list; the folder stays on disk.
  pub fn remove(&mut self, path: &Path) {
    self.projects.retain(|project| project.path != path);
    if self.active.as_deref() == Some(path) {
      self.active = self.projects.first().map(|project| project.path.clone());
    }
  }
}
