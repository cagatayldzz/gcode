use crate::data_dir;
use serde_json::{json, Value};
use std::{
  io,
  path::{Path, PathBuf},
};

/// How many files and projects the list keeps of each.
const KEPT: usize = 20;

/// A file or project that was used lately, and the profile it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentEntry {
  pub profile: String,
  pub path: PathBuf,
}

/// The files that were edited and the projects that were opened last, newest first (`recent.json`). It is shared by the profiles, so that
/// the Dock menu can offer them all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentList {
  pub files: Vec<RecentEntry>,
  pub projects: Vec<RecentEntry>,
}

fn touch(entries: &mut Vec<RecentEntry>, profile: &str, path: &Path) {
  entries.retain(|entry| !(entry.profile == profile && entry.path == path));
  entries.insert(0, RecentEntry { profile: profile.to_string(), path: path.to_path_buf() });
  entries.truncate(KEPT);
}

impl RecentList {
  pub fn default_path() -> PathBuf {
    data_dir().join("recent.json")
  }

  /// Reads the list; a missing or damaged file gives an empty list.
  pub fn load(path: &Path) -> Self {
    let Some(Value::Object(root)) = crate::read_json(path) else {
      return Self::default();
    };
    let read = |key: &str| -> Vec<RecentEntry> {
      root
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
          Some(RecentEntry { profile: item.get("profile")?.as_str()?.to_string(), path: PathBuf::from(item.get("path")?.as_str()?) })
        })
        .take(KEPT)
        .collect()
    };
    Self { files: read("files"), projects: read("projects") }
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    let entries = |entries: &[RecentEntry]| {
      entries.iter().map(|entry| json!({ "profile": entry.profile, "path": entry.path.to_string_lossy() })).collect::<Vec<_>>()
    };
    crate::write_json(path, &json!({ "files": entries(&self.files), "projects": entries(&self.projects) }))
  }

  /// Puts a file first: it was just edited.
  pub fn touch_file(&mut self, profile: &str, path: &Path) {
    touch(&mut self.files, profile, path);
  }

  /// Puts a project first: it was just opened.
  pub fn touch_project(&mut self, profile: &str, path: &Path) {
    touch(&mut self.projects, profile, path);
  }

  /// Forgets a project and the files inside it, for example when the project is removed from its profile.
  pub fn forget_project(&mut self, profile: &str, path: &Path) {
    self.projects.retain(|entry| !(entry.profile == profile && entry.path == path));
    self.files.retain(|entry| !(entry.profile == profile && entry.path.starts_with(path)));
  }

  /// The files and the projects to offer in a menu of `total` places: the projects are never fewer than `min_projects` (when there are that
  /// many), and the files take the places left, so that a long list of files does not push the projects out.
  pub fn menu_entries(&self, total: usize, min_projects: usize) -> (&[RecentEntry], &[RecentEntry]) {
    let files = self.files.len().min(total.saturating_sub(self.projects.len().min(min_projects)));
    let projects = self.projects.len().min(total - files);
    (&self.files[..files], &self.projects[..projects])
  }

  /// Forgets everything of a profile that was deleted.
  pub fn forget_profile(&mut self, profile: &str) {
    self.files.retain(|entry| entry.profile != profile);
    self.projects.retain(|entry| entry.profile != profile);
  }
}
