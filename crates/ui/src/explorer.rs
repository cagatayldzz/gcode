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
  /// Git ignores the entry (or a folder around it).
  pub ignored: bool,
}

/// Names that are never shown.
const HIDDEN: [&str; 2] = [".git", ".DS_Store"];

/// The rows to show for `root`: its children, and the children of every folder in `expanded`,
/// folders before files, each group sorted by name as GitHub does: by bytes, so a capital letter comes before a lowercase one.
pub(crate) fn rows(root: &Path, expanded: &HashSet<PathBuf>) -> Vec<Row> {
  let mut out = Vec::new();
  let rules = ws_git::IgnoreRules::open(root);
  collect(root, 0, false, rules.as_ref(), expanded, &mut out);
  out
}

fn collect(dir: &Path, depth: usize, inherited: bool, rules: Option<&ws_git::IgnoreRules>, expanded: &HashSet<PathBuf>, out: &mut Vec<Row>) {
  let Ok(entries) = fs::read_dir(dir) else { return };
  let mut children: Vec<(bool, String, Row)> = entries
    .filter_map(Result::ok)
    .filter_map(|entry| {
      let name = entry.file_name().to_string_lossy().into_owned();
      if HIDDEN.contains(&name.as_str()) {
        return None;
      }
      let path = entry.path();
      let is_dir = match entry.file_type() {
        Ok(kind) if !kind.is_symlink() => kind.is_dir(),
        _ => path.is_dir(),
      };
      let ignored = inherited || rules.is_some_and(|rules| rules.is_ignored(&path));
      Some((!is_dir, name.clone(), Row { is_dir, path, name, depth, ignored }))
    })
    .collect();
  children.sort_unstable_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
  for (_, _, row) in children {
    if row.is_dir && expanded.contains(&row.path) {
      let (path, ignored) = (row.path.clone(), row.ignored);
      out.push(row);
      collect(&path, depth + 1, ignored, rules, expanded, out);
    } else {
      out.push(row);
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
  if metadata.is_dir() {
    return copy_directory(source, target, metadata);
  }
  let mut input = fs::File::open(source)?;
  let mut output = fs::File::create_new(target)?;
  let result = std::io::copy(&mut input, &mut output).and_then(|_| output.set_permissions(metadata.permissions()));
  if result.is_err() {
    let _ = fs::remove_file(target);
  }
  result
}

fn copy_directory(source: &Path, target: &Path, metadata: &fs::Metadata) -> std::io::Result<()> {
  if target.starts_with(source) {
    return Err(std::io::Error::other("A folder cannot be copied into itself."));
  }
  fs::create_dir(target)?;
  let result = (|| {
    for entry in fs::read_dir(source)? {
      let entry = entry?;
      let child = fs::symlink_metadata(entry.path())?;
      copy_to_new_file(&entry.path(), &target.join(entry.file_name()), &child)?;
    }
    fs::set_permissions(target, metadata.permissions())
  })();
  if result.is_err() {
    let _ = fs::remove_dir_all(target);
  }
  result
}
