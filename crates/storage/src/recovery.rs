use crate::{files::temporary_for, FileVersion};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
  ffi::OsString,
  fs::{self, DirBuilder, File},
  io,
  os::unix::{ffi::OsStringExt as _, fs::DirBuilderExt as _},
  path::{Path, PathBuf},
};

/// Unsaved text and the disk version it was edited from.
#[derive(Debug, Deserialize, Serialize)]
pub struct Draft {
  path: Vec<u8>,
  pub text: String,
  pub base: Option<FileVersion>,
}

impl Draft {
  pub fn new(path: &Path, text: String, base: Option<FileVersion>) -> Self {
    Self { path: path.as_os_str().as_encoded_bytes().to_vec(), text, base }
  }

  pub fn path(&self) -> PathBuf {
    OsString::from_vec(self.path.clone()).into()
  }
}

/// Independent, atomic snapshots in one profile's private directory.
pub struct RecoveryJournal(PathBuf);

impl RecoveryJournal {
  pub fn new(profile_dir: &Path) -> Self {
    Self(profile_dir.join("editor-recovery"))
  }

  pub fn load(&self) -> io::Result<(Vec<Draft>, usize)> {
    let entries = match fs::read_dir(&self.0) {
      Ok(entries) => entries,
      Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
      Err(error) => return Err(error),
    };
    let mut drafts = Vec::new();
    let mut unreadable = 0;
    for entry in entries {
      let Ok(entry) = entry else {
        unreadable += 1;
        continue;
      };
      let path = entry.path();
      if path.extension().is_none_or(|extension| extension != "json") {
        continue;
      }
      if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
        unreadable += 1;
        continue;
      }
      let draft = File::open(&path).and_then(|file| serde_json::from_reader::<_, Draft>(io::BufReader::new(file)).map_err(io::Error::from));
      match draft {
        Ok(draft) if draft.path().is_absolute() && path == self.entry(&draft.path()) => drafts.push(draft),
        _ => unreadable += 1,
      }
    }
    drafts.sort_by(|left, right| left.path.cmp(&right.path));
    Ok((drafts, unreadable))
  }

  pub fn store(&self, draft: &Draft) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(&self.0)?;
    let path = self.entry(&draft.path());
    let mut temporary = temporary_for(&path, false)?;
    serde_json::to_writer(&mut temporary, draft)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
  }

  pub fn remove(&self, path: &Path) -> io::Result<()> {
    match fs::remove_file(self.entry(path)) {
      Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
      result => result,
    }
  }

  fn entry(&self, path: &Path) -> PathBuf {
    self.0.join(format!("{:x}.json", Sha256::digest(path.as_os_str().as_encoded_bytes())))
  }
}
