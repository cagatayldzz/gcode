use serde_json::{json, Value};
use std::{
  fs, io,
  path::{Path, PathBuf},
};

/// A folder the user works in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
  pub path: PathBuf,
  /// The id of the category the project is in; `None` for a project outside any category.
  pub category: Option<String>,
  /// The name shown instead of the folder's name.
  pub name: Option<String>,
  /// The name of the icon shown instead of the first letter.
  pub icon: Option<String>,
  /// The color of the avatar, as `#rrggbb`.
  pub color: Option<String>,
  /// Starred by the user: listed first in the project menu.
  pub favorite: bool,
  /// Branches the user starred in this project: listed first in the branch menu. Kept here, not in the repository.
  pub favorite_branches: Vec<String>,
}

impl Project {
  pub fn new(path: PathBuf) -> Self {
    Self { path, category: None, name: None, icon: None, color: None, favorite: false, favorite_branches: Vec::new() }
  }

  /// The folder's name.
  pub fn folder_name(&self) -> String {
    self.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| self.path.display().to_string())
  }

  /// The name shown in the interface: the one the user chose, or the folder's.
  pub fn name(&self) -> String {
    self.name.clone().unwrap_or_else(|| self.folder_name())
  }
}

/// A group of projects in the rail, like a folder of servers in Discord.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Category {
  pub id: String,
  /// Empty for a category the user has not named.
  pub name: String,
  /// Shown as one tile instead of its projects.
  pub collapsed: bool,
}

/// The projects in the sidebar and the one that was open last (`projects.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectList {
  pub projects: Vec<Project>,
  pub categories: Vec<Category>,
  pub active: Option<PathBuf>,
}

impl ProjectList {
  /// Reads the list; a missing or damaged file gives an empty list.
  pub fn load(path: &Path) -> Self {
    let Some(Value::Object(root)) = crate::read_json(path) else {
      return Self::default();
    };
    // A project is a path (older files) or an object.
    let text = |object: &serde_json::Map<String, Value>, key: &str| object.get(key).and_then(Value::as_str).map(str::to_owned);
    let projects: Vec<Project> = root
      .get("projects")
      .and_then(Value::as_array)
      .map(|items| {
        items
          .iter()
          .filter_map(|item| match item {
            Value::String(path) => Some(Project::new(PathBuf::from(path))),
            Value::Object(object) => Some(Project {
              path: PathBuf::from(object.get("path")?.as_str()?),
              category: text(object, "category"),
              name: text(object, "name").filter(|name| !name.is_empty()),
              icon: text(object, "icon"),
              color: text(object, "color"),
              favorite: object.get("favorite").and_then(Value::as_bool).unwrap_or(false),
              favorite_branches: object
                .get("favorite_branches")
                .and_then(Value::as_array)
                .map(|names| names.iter().filter_map(|name| name.as_str().map(str::to_owned)).collect())
                .unwrap_or_default(),
            }),
            _ => None,
          })
          .collect()
      })
      .unwrap_or_default();
    let mut categories: Vec<Category> = Vec::new();
    for item in root.get("categories").and_then(Value::as_array).into_iter().flatten() {
      let Value::Object(object) = item else { continue };
      let name = text(object, "name").unwrap_or_default();
      let Some(id) = text(object, "id").or_else(|| Some(name.clone()).filter(|name| !name.is_empty())) else { continue };
      if !categories.iter().any(|category| category.id == id) {
        categories.push(Category { id, name, collapsed: object.get("collapsed").and_then(Value::as_bool).unwrap_or(false) });
      }
    }
    let active = root.get("active").and_then(Value::as_str).map(PathBuf::from);
    let mut list = Self { projects, categories, active };
    // A project never points at a category that does not exist.
    for project in &mut list.projects {
      if project.category.as_ref().is_some_and(|id| !list.categories.iter().any(|category| &category.id == id)) {
        project.category = None;
      }
    }
    list
  }

  pub fn save(&self, path: &Path) -> io::Result<()> {
    let value = json!({
      "projects": self.projects.iter().map(|project| json!({
          "path": project.path.to_string_lossy(),
          "category": project.category,
          "name": project.name,
          "icon": project.icon,
          "color": project.color,
          "favorite": project.favorite,
          "favorite_branches": project.favorite_branches,
        })).collect::<Vec<_>>(),
      "categories": self.categories.iter().map(|category| json!({ "id": category.id, "name": category.name, "collapsed": category.collapsed })).collect::<Vec<_>>(),
      "active": self.active.as_ref().map(|path| path.to_string_lossy()),
    });
    crate::write_json(path, &value)
  }

  /// Adds a folder (once) and makes it the active project.
  pub fn add(&mut self, path: PathBuf) {
    if !self.projects.iter().any(|project| project.path == path) {
      self.projects.push(Project::new(path.clone()));
    }
    self.active = Some(path);
  }

  /// Adds folders that are not listed yet, keeping the project that is open. With none open, the first one opens.
  /// Returns how many were added.
  pub fn add_all(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> usize {
    let before = self.projects.len();
    for path in paths {
      if !self.projects.iter().any(|project| project.path == path) {
        self.projects.push(Project::new(path));
      }
    }
    if self.active.is_none() {
      self.active = self.projects.first().map(|project| project.path.clone());
    }
    self.projects.len() - before
  }

  /// The projects whose folder is gone. A folder that cannot be read for another reason (no permission, a disk that is
  /// not mounted) is not counted: only a folder that is certainly missing.
  pub fn missing_paths(&self) -> Vec<PathBuf> {
    self
      .projects
      .iter()
      .filter(|project| matches!(fs::metadata(&project.path), Err(error) if error.kind() == io::ErrorKind::NotFound))
      .map(|project| project.path.clone())
      .collect()
  }

  /// Removes a project from the list; the folder stays on disk.
  pub fn remove(&mut self, path: &Path) {
    self.projects.retain(|project| project.path != path);
    if self.active.as_deref() == Some(path) {
      self.active = self.projects.first().map(|project| project.path.clone());
    }
  }

  /// Points a project at its new folder after the folder was moved, keeping its look, category, favorite and place.
  /// Returns false when `from` is not listed or `to` already is.
  pub fn relocate(&mut self, from: &Path, to: &Path) -> bool {
    if self.projects.iter().any(|project| project.path == to) {
      return false;
    }
    let Some(project) = self.projects.iter_mut().find(|project| project.path == from) else { return false };
    project.path = to.to_path_buf();
    if self.active.as_deref() == Some(from) {
      self.active = Some(to.to_path_buf());
    }
    true
  }

  /// Stars or unstars a project.
  pub fn set_favorite(&mut self, path: &Path, favorite: bool) {
    if let Some(project) = self.projects.iter_mut().find(|project| project.path == path) {
      project.favorite = favorite;
    }
  }

  /// Stars or unstars a branch of a project.
  pub fn set_favorite_branch(&mut self, path: &Path, branch: &str, favorite: bool) {
    let Some(project) = self.projects.iter_mut().find(|project| project.path == path) else { return };
    project.favorite_branches.retain(|name| name != branch);
    if favorite {
      project.favorite_branches.push(branch.to_string());
    }
  }

  /// The starred branches of a project.
  pub fn favorite_branches(&self, path: &Path) -> &[String] {
    self.projects.iter().find(|project| project.path == path).map_or(&[], |project| project.favorite_branches.as_slice())
  }

  /// The starred projects, in list order.
  pub fn favorites(&self) -> impl Iterator<Item = &Project> {
    self.projects.iter().filter(|project| project.favorite)
  }

  /// The projects in the category with this id, or outside any category for `None`, in list order.
  pub fn projects_in<'a>(&'a self, category: Option<&'a str>) -> impl Iterator<Item = &'a Project> {
    self.projects.iter().filter(move |project| project.category.as_deref() == category)
  }

  /// Adds an empty category, named or not, and returns its id.
  pub fn add_category(&mut self, name: &str) -> String {
    let next = self.categories.iter().filter_map(|category| category.id.strip_prefix("category-")?.parse::<u64>().ok()).max().unwrap_or(0) + 1;
    let id = format!("category-{next}");
    self.categories.push(Category { id: id.clone(), name: name.trim().to_owned(), collapsed: false });
    id
  }

  /// Sets a category's name; an empty name makes it unnamed.
  pub fn rename_category(&mut self, id: &str, name: &str) {
    if let Some(category) = self.categories.iter_mut().find(|category| category.id == id) {
      category.name = name.trim().to_owned();
    }
  }

  /// Deletes a category; its projects stay in the list, outside any category.
  pub fn remove_category(&mut self, id: &str) {
    self.categories.retain(|category| category.id != id);
    for project in self.projects.iter_mut().filter(|project| project.category.as_deref() == Some(id)) {
      project.category = None;
    }
  }

  /// Puts a project in a category, or takes it out with `None`. An unknown category leaves the project where it is.
  /// A category left without projects is deleted.
  pub fn set_category(&mut self, path: &Path, category: Option<&str>) {
    if category.is_some_and(|id| !self.categories.iter().any(|existing| existing.id == id)) {
      return;
    }
    if let Some(project) = self.projects.iter_mut().find(|project| project.path == path) {
      project.category = category.map(str::to_owned);
    }
    self.prune_empty_categories();
  }

  /// Drops `dragged` onto `target`: it joins the category `target` is in, or the two start a new unnamed category.
  /// Returns the id of that category, or `None` when either is not listed or they are the same project.
  pub fn group(&mut self, dragged: &Path, target: &Path) -> Option<String> {
    if dragged == target || !self.projects.iter().any(|project| project.path == dragged) {
      return None;
    }
    let existing = self.projects.iter().find(|project| project.path == target)?.category.clone();
    let id = existing.unwrap_or_else(|| {
      let id = self.add_category("");
      self.set_category(target, Some(&id));
      id
    });
    self.set_category(dragged, Some(&id));
    Some(id)
  }

  /// Folds or unfolds a category.
  pub fn toggle_category(&mut self, id: &str) {
    if let Some(category) = self.categories.iter_mut().find(|category| category.id == id) {
      category.collapsed = !category.collapsed;
    }
  }

  /// Sets how a project looks in the rail. An empty name, icon or color goes back to the default.
  pub fn set_appearance(&mut self, path: &Path, name: &str, icon: Option<&str>, color: Option<&str>) {
    if let Some(project) = self.projects.iter_mut().find(|project| project.path == path) {
      project.name = Some(name.trim().to_owned()).filter(|name| !name.is_empty() && *name != project.folder_name());
      project.icon = icon.map(str::to_owned);
      project.color = color.map(str::to_owned);
    }
  }

  /// Moves a project up (`-1`) or down (`1`) among the projects of its own category, which is the order the rail shows.
  pub fn move_project_by(&mut self, path: &Path, delta: isize) {
    let Some(index) = self.projects.iter().position(|project| project.path == path) else { return };
    let category = self.projects[index].category.clone();
    let siblings: Vec<usize> = (0..self.projects.len()).filter(|&other| self.projects[other].category == category).collect();
    let Some(place) = siblings.iter().position(|&other| other == index) else { return };
    let Some(&neighbor) = place.checked_add_signed(delta).and_then(|target| siblings.get(target)) else { return };
    self.projects.swap(index, neighbor);
  }

  /// Moves a category up (`-1`) or down (`1`) among the categories.
  pub fn move_category_by(&mut self, id: &str, delta: isize) {
    let Some(index) = self.categories.iter().position(|category| category.id == id) else { return };
    if let Some(target) = index.checked_add_signed(delta).filter(|&target| target < self.categories.len()) {
      self.categories.swap(index, target);
    }
  }

  /// Puts `dragged` right before `target` in the list, in the category of `target`. Returns false when either is not
  /// listed or they are the same project.
  pub fn move_project_before(&mut self, dragged: &Path, target: &Path) -> bool {
    if dragged == target {
      return false;
    }
    let (Some(from), Some(_)) =
      (self.projects.iter().position(|project| project.path == dragged), self.projects.iter().position(|project| project.path == target))
    else {
      return false;
    };
    let mut project = self.projects.remove(from);
    let to = self.projects.iter().position(|other| other.path == target).unwrap_or(self.projects.len());
    project.category = self.projects[to].category.clone();
    self.projects.insert(to, project);
    self.prune_empty_categories();
    true
  }

  /// Swaps the places of two projects in the list without touching their categories: the project bar's live reordering.
  /// Returns false when either is not listed or they are the same project.
  pub fn swap_projects(&mut self, first: &Path, second: &Path) -> bool {
    let (Some(a), Some(b)) =
      (self.projects.iter().position(|project| project.path == first), self.projects.iter().position(|project| project.path == second))
    else {
      return false;
    };
    if a == b {
      return false;
    }
    self.projects.swap(a, b);
    true
  }

  /// Deletes the unnamed categories without projects: the ones that dragging made, and nobody asked for by name. A
  /// category the user named stays until it is deleted, empty or not.
  fn prune_empty_categories(&mut self) {
    let projects = &self.projects;
    self
      .categories
      .retain(|category| !category.name.is_empty() || projects.iter().any(|project| project.category.as_deref() == Some(category.id.as_str())));
  }
}
