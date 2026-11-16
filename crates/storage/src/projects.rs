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
  /// The name of the category the project is in; `None` for a project outside any category.
  pub category: Option<String>,
}

/// A named group of projects in the rail, like a folder of servers in Discord.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Category {
  pub name: String,
  /// Shown as one tile instead of its projects.
  pub collapsed: bool,
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
  pub categories: Vec<Category>,
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
    // A project is a path (older files) or an object with a path and a category.
    let projects: Vec<Project> = root
      .get("projects")
      .and_then(Value::as_array)
      .map(|items| {
        items
          .iter()
          .filter_map(|item| match item {
            Value::String(path) => Some(Project { path: PathBuf::from(path), category: None }),
            Value::Object(object) => Some(Project {
              path: PathBuf::from(object.get("path")?.as_str()?),
              category: object.get("category").and_then(Value::as_str).map(str::to_owned),
            }),
            _ => None,
          })
          .collect()
      })
      .unwrap_or_default();
    let mut categories: Vec<Category> = Vec::new();
    for item in root.get("categories").and_then(Value::as_array).into_iter().flatten() {
      let Some(name) = item.get("name").and_then(Value::as_str) else { continue };
      if !name.is_empty() && !categories.iter().any(|category| category.name == name) {
        categories.push(Category { name: name.to_owned(), collapsed: item.get("collapsed").and_then(Value::as_bool).unwrap_or(false) });
      }
    }
    let active = root.get("active").and_then(Value::as_str).map(PathBuf::from);
    let mut list = Self { projects, categories, active };
    // A project never points at a category that does not exist.
    for project in &mut list.projects {
      if project.category.as_ref().is_some_and(|name| !list.categories.iter().any(|category| &category.name == name)) {
        project.category = None;
      }
    }
    list
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    let value = json!({
      "projects": self.projects.iter().map(|project| json!({ "path": project.path.to_string_lossy(), "category": project.category })).collect::<Vec<_>>(),
      "categories": self.categories.iter().map(|category| json!({ "name": category.name, "collapsed": category.collapsed })).collect::<Vec<_>>(),
      "active": self.active.as_ref().map(|path| path.to_string_lossy()),
    });
    fs::write(path, serde_json::to_string_pretty(&value)?)
  }

  /// Adds a folder (once) and makes it the active project.
  pub fn add(&mut self, path: PathBuf) {
    if !self.projects.iter().any(|project| project.path == path) {
      self.projects.push(Project { path: path.clone(), category: None });
    }
    self.active = Some(path);
  }

  /// Adds folders that are not listed yet, keeping the project that is open. With none open, the first one opens.
  /// Returns how many were added.
  pub fn add_all(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> usize {
    let before = self.projects.len();
    for path in paths {
      if !self.projects.iter().any(|project| project.path == path) {
        self.projects.push(Project { path, category: None });
      }
    }
    if self.active.is_none() {
      self.active = self.projects.first().map(|project| project.path.clone());
    }
    self.projects.len() - before
  }

  /// Removes a project from the list; the folder stays on disk.
  pub fn remove(&mut self, path: &Path) {
    self.projects.retain(|project| project.path != path);
    if self.active.as_deref() == Some(path) {
      self.active = self.projects.first().map(|project| project.path.clone());
    }
  }

  /// The projects in `category`, or outside any category for `None`, in list order.
  pub fn projects_in<'a>(&'a self, category: Option<&'a str>) -> impl Iterator<Item = &'a Project> {
    self.projects.iter().filter(move |project| project.category.as_deref() == category)
  }

  /// Adds an empty category. Returns false for an empty or a taken name.
  pub fn add_category(&mut self, name: &str) -> bool {
    let name = name.trim();
    if name.is_empty() || self.categories.iter().any(|category| category.name == name) {
      return false;
    }
    self.categories.push(Category { name: name.to_owned(), collapsed: false });
    true
  }

  /// Renames a category, keeping its projects. Returns false for an empty or a taken name.
  pub fn rename_category(&mut self, old: &str, new: &str) -> bool {
    let new = new.trim();
    if new.is_empty() || (new != old && self.categories.iter().any(|category| category.name == new)) {
      return false;
    }
    let Some(category) = self.categories.iter_mut().find(|category| category.name == old) else { return false };
    category.name = new.to_owned();
    for project in self.projects.iter_mut().filter(|project| project.category.as_deref() == Some(old)) {
      project.category = Some(new.to_owned());
    }
    true
  }

  /// Deletes a category; its projects stay in the list, outside any category.
  pub fn remove_category(&mut self, name: &str) {
    self.categories.retain(|category| category.name != name);
    for project in self.projects.iter_mut().filter(|project| project.category.as_deref() == Some(name)) {
      project.category = None;
    }
  }

  /// Puts a project in a category, or takes it out with `None`. An unknown category leaves the project where it is.
  pub fn set_category(&mut self, path: &Path, category: Option<&str>) {
    if category.is_some_and(|name| !self.categories.iter().any(|existing| existing.name == name)) {
      return;
    }
    if let Some(project) = self.projects.iter_mut().find(|project| project.path == path) {
      project.category = category.map(str::to_owned);
    }
  }

  /// Folds or unfolds a category.
  pub fn toggle_category(&mut self, name: &str) {
    if let Some(category) = self.categories.iter_mut().find(|category| category.name == name) {
      category.collapsed = !category.collapsed;
    }
  }
}
