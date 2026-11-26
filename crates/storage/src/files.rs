use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::{
  ffi::OsString,
  fs,
  io::{self, Read, Write},
  path::{Path, PathBuf},
  sync::Mutex,
};
use tempfile::Builder;

/// Writes `bytes` to `path` so that a crash or a full disk never leaves half a file: the bytes go to a temporary file beside
/// the target, reach the disk, and then replace the target in one step.
///
/// A symbolic link is followed, so the link stays a link. The permissions of an existing file are kept.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
  write_atomic_checked(path, bytes, |_| Ok(true)).map(|_| ())
}

/// The contents and resolved destination a file had when it was read. A missing file is tracked separately.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileVersion {
  target: Vec<u8>,
  digest: [u8; 32],
}

impl FileVersion {
  pub fn from_contents(path: &Path, bytes: &[u8]) -> Self {
    let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Self { target: target.as_os_str().as_encoded_bytes().to_vec(), digest: Sha256::digest(bytes).into() }
  }

  pub fn with_contents(&self, bytes: &[u8]) -> Self {
    Self { target: self.target.clone(), digest: Sha256::digest(bytes).into() }
  }

  pub fn read(path: &Path) -> io::Result<Option<Self>> {
    let target = match path.canonicalize() {
      Ok(target) => target,
      Err(error) if error.kind() == io::ErrorKind::NotFound => match fs::symlink_metadata(path) {
        Err(missing) if missing.kind() == io::ErrorKind::NotFound => return Ok(None),
        _ => return Err(error),
      },
      Err(error) => return Err(error),
    };
    if !fs::metadata(&target)?.is_file() {
      return Err(io::Error::new(io::ErrorKind::InvalidInput, "The destination is not a regular file."));
    }
    let mut file = fs::File::open(&target)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
      let count = file.read(&mut buffer)?;
      if count == 0 {
        break;
      }
      hash.update(&buffer[..count]);
    }
    Ok(Some(Self { target: target.as_os_str().as_encoded_bytes().to_vec(), digest: hash.finalize().into() }))
  }
}

/// Rechecks the disk version after staging the write and immediately before replacing the destination.
pub fn write_if_current(path: &Path, bytes: &[u8], expected: Option<&FileVersion>) -> io::Result<bool> {
  // Separate profile windows may edit the same file. Serialize their check and replacement.
  static WRITES: Mutex<()> = Mutex::new(());
  let _write = WRITES.lock().map_err(|_| io::Error::other("A previous editor write was interrupted."))?;
  if expected.is_none() {
    return match write_new(path, bytes) {
      Ok(()) => Ok(true),
      Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
      Err(error) => Err(error),
    };
  }
  write_atomic_checked(path, bytes, |target| {
    let current = FileVersion::read(path)?;
    Ok(current.as_ref() == expected && current.as_ref().is_some_and(|version| version.target == target.as_os_str().as_encoded_bytes()))
  })
}

fn write_atomic_checked(path: &Path, bytes: &[u8], check: impl FnOnce(&Path) -> io::Result<bool>) -> io::Result<bool> {
  let (target, permissions) = match fs::symlink_metadata(path) {
    Ok(_) => {
      let target = fs::canonicalize(path)?;
      let permissions = fs::metadata(&target)?.permissions();
      (target, Some(permissions))
    }
    Err(error) if error.kind() == io::ErrorKind::NotFound => (path.to_path_buf(), None),
    Err(error) => return Err(error),
  };
  let mut temporary = temporary_for(&target, permissions.is_none())?;
  temporary.write_all(bytes)?;
  if let Some(permissions) = permissions {
    temporary.as_file().set_permissions(permissions)?;
  }
  temporary.as_file().sync_all()?;
  if !check(&target)? {
    return Ok(false);
  }
  temporary.persist(target).map_err(|error| error.error)?;
  Ok(true)
}

/// Creates a new file atomically and refuses to replace a destination that already exists.
pub fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
  let mut temporary = temporary_for(path, true)?;
  temporary.write_all(bytes)?;
  temporary.as_file().sync_all()?;
  temporary.persist_noclobber(path).map_err(|error| error.error)?;
  Ok(())
}

pub(crate) fn temporary_for(path: &Path, public: bool) -> io::Result<tempfile::NamedTempFile> {
  let parent = path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new("."));
  let mut builder = Builder::new();
  if public {
    use std::os::unix::fs::PermissionsExt;

    builder.permissions(fs::Permissions::from_mode(0o666));
  }
  builder.tempfile_in(parent)
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
