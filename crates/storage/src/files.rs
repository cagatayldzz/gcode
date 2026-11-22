use serde_json::Value;
use std::{
  ffi::OsString,
  fs::{self, File},
  io::{self, Write},
  path::{Path, PathBuf},
};

/// Writes `bytes` to `path` so that a crash or a full disk never leaves half a file: the bytes go to a temporary file beside
/// the target, reach the disk, and then replace the target in one step.
///
/// A symbolic link is followed, so the link stays a link. The permissions of an existing file are kept.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
  let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
  let temporary = sibling(&target, ".vekro-tmp");
  let result = (|| {
    let mut file = File::create(&temporary)?;
    if let Ok(metadata) = fs::metadata(&target) {
      fs::set_permissions(&temporary, metadata.permissions())?;
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, &target)
  })();
  if result.is_err() {
    let _ = fs::remove_file(&temporary);
  }
  result
}

/// Writes a JSON file atomically. The file it replaces stays beside it as `<name>.bak`, so a bad write can be recovered.
pub fn write_json(path: &Path, value: &Value) -> io::Result<()> {
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent)?;
  }
  let text = serde_json::to_string_pretty(value)?;
  if path.is_file() {
    // The backup is best effort: a failed copy must not stop the save.
    let _ = fs::copy(path, sibling(path, ".bak"));
  }
  write_atomic(path, text.as_bytes())
}

/// Reads a JSON file. A file that is not valid JSON is kept as `<name>.corrupt` and the last backup is read instead; a
/// missing file gives `None`.
pub fn read_json(path: &Path) -> Option<Value> {
  let text = fs::read_to_string(path).ok()?;
  match serde_json::from_str(&text) {
    Ok(value) => Some(value),
    Err(_) => {
      let _ = fs::rename(path, sibling(path, ".corrupt"));
      let value = serde_json::from_str(&fs::read_to_string(sibling(path, ".bak")).ok()?).ok()?;
      Some(value)
    }
  }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
  let mut name: OsString = path.file_name().map(OsString::from).unwrap_or_default();
  name.push(suffix);
  path.with_file_name(name)
}
