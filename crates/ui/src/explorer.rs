//! The folder tree of the open project.

use std::{
  collections::HashSet,
  fs,
  path::{Component, Path, PathBuf},
};

/// One visible line of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
  pub path: PathBuf,
  pub name: String,
  pub depth: usize,
  pub is_dir: bool,
}

/// Names that are never shown.
const HIDDEN: [&str; 2] = [".git", ".DS_Store"];

/// The rows to show for `root`: its children, and the children of every folder in `expanded`,
/// folders before files, each group sorted by name.
pub(crate) fn rows(root: &Path, expanded: &HashSet<PathBuf>) -> Vec<Row> {
  let mut out = Vec::new();
  collect(root, 0, expanded, &mut out);
  out
}

fn collect(dir: &Path, depth: usize, expanded: &HashSet<PathBuf>, out: &mut Vec<Row>) {
  let Ok(entries) = fs::read_dir(dir) else { return };
  let mut children: Vec<Row> = entries
    .filter_map(Result::ok)
    .filter_map(|entry| {
      let name = entry.file_name().to_string_lossy().into_owned();
      (!HIDDEN.contains(&name.as_str())).then(|| Row { is_dir: entry.path().is_dir(), path: entry.path(), name, depth })
    })
    .collect();
  children.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
  for row in children {
    let open = row.is_dir && expanded.contains(&row.path);
    let path = row.path.clone();
    out.push(row);
    if open {
      collect(&path, depth + 1, expanded, out);
    }
  }
}

/// Creates an empty file or a folder called `name` inside `parent`. `name` may hold subfolders (`a/b.rs`); it may not
/// leave `parent` or replace something that exists. Returns the new path.
pub(crate) fn create(parent: &Path, name: &str, is_dir: bool) -> Result<PathBuf, String> {
  let name = name.trim();
  if name.is_empty() {
    return Err("Enter a name.".into());
  }
  let relative = Path::new(name);
  if relative.is_absolute() || relative.components().any(|part| !matches!(part, Component::Normal(_))) {
    return Err("The name must stay inside the folder.".into());
  }
  let path = parent.join(relative);
  if path.exists() {
    return Err(format!("{name} already exists."));
  }
  let made = if is_dir {
    fs::create_dir_all(&path)
  } else {
    path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| fs::File::create_new(&path).map(drop))
  };
  made.map(|()| path).map_err(|error| format!("Could not create {name}: {error}"))
}
