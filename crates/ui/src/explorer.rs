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

/// Moves a file without replacing an existing destination.
pub(crate) fn move_file(source: &Path, target: &Path) -> std::io::Result<()> {
  use rustix::fs::{renameat_with, RenameFlags, CWD};
  renameat_with(CWD, source, CWD, target, RenameFlags::NOREPLACE)?;
  Ok(())
}

/// Copies a file without replacing an existing destination. Copies within the same folder receive a new name.
pub(crate) fn copy_file(source: &Path, target: &Path, metadata: &fs::Metadata, duplicate: bool) -> std::io::Result<PathBuf> {
  if !duplicate {
    copy_to_new_file(source, target, metadata)?;
    return Ok(target.to_path_buf());
  }

  for index in 1..=10_000 {
    let candidate = duplicate_path(target, index);
    match copy_to_new_file(source, &candidate, metadata) {
      Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
      result => return result.map(|()| candidate),
    }
  }
  Err(std::io::Error::other("No available name for a copy."))
}

fn duplicate_path(path: &Path, index: u32) -> PathBuf {
  let mut name = path.file_stem().unwrap_or_default().to_os_string();
  name.push(" copy");
  if index > 1 {
    name.push(format!(" {index}"));
  }
  if let Some(extension) = path.extension() {
    name.push(".");
    name.push(extension);
  }
  path.with_file_name(name)
}

fn copy_to_new_file(source: &Path, target: &Path, metadata: &fs::Metadata) -> std::io::Result<()> {
  if metadata.is_symlink() {
    return std::os::unix::fs::symlink(fs::read_link(source)?, target);
  }
  let mut input = fs::File::open(source)?;
  let mut output = fs::File::create_new(target)?;
  let result = std::io::copy(&mut input, &mut output).and_then(|_| output.set_permissions(metadata.permissions()));
  if result.is_err() {
    let _ = fs::remove_file(target);
  }
  result
}
