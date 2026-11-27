//! Durable records of agent worktrees, stored beside their checkouts.

use crate::Profile;
use serde::{Deserialize, Serialize};
use std::{
  fs, io,
  path::{Path, PathBuf},
  time::{SystemTime, UNIX_EPOCH},
};

const RECORD: &str = "session.json";
const CHECKOUT: &str = "checkout";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentWorktree {
  pub source: PathBuf,
  pub agent: String,
  pub branch: String,
  pub base: String,
  pub created: u64,
}

impl AgentWorktree {
  pub fn reserve(profile: &Profile, source: &Path, agent: &str, base: String) -> io::Result<(PathBuf, Self)> {
    let parent = profile.agent_worktrees_path();
    fs::create_dir_all(&parent)?;
    let directory = tempfile::Builder::new().prefix(&format!("{agent}-")).tempdir_in(parent)?;
    let source = source.canonicalize()?;
    let created = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let name = directory.path().file_name().expect("temporary folder has a name").to_string_lossy();
    let record = Self { source, agent: agent.into(), branch: format!("agent/{name}"), base, created };
    crate::write_atomic(&directory.path().join(RECORD), &serde_json::to_vec_pretty(&record)?)?;
    Ok((directory.keep().join(CHECKOUT), record))
  }

  pub fn list(profile: &Profile, source: &Path) -> io::Result<Vec<(PathBuf, Self)>> {
    let source = source.canonicalize()?;
    let directory = match fs::read_dir(profile.agent_worktrees_path()) {
      Ok(directory) => directory,
      Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
      Err(error) => return Err(error),
    };
    let mut records = Vec::new();
    for entry in directory {
      let entry = entry?;
      if !entry.file_type()?.is_dir() {
        continue;
      }
      let checkout = entry.path().join(CHECKOUT);
      if !checkout.join(".git").is_file() {
        continue;
      }
      let record: Self = serde_json::from_slice(&fs::read(entry.path().join(RECORD))?)?;
      let checkout = checkout.canonicalize()?;
      if record.source == source || checkout == source {
        records.push((checkout, record));
      }
    }
    records.sort_by(|(left_path, left), (right_path, right)| right.created.cmp(&left.created).then_with(|| left_path.cmp(right_path)));
    Ok(records)
  }

  /// Releases a reservation only if Git left no working directory behind.
  pub fn release_empty(checkout: &Path) {
    if !matches!(checkout.symlink_metadata(), Err(error) if error.kind() == io::ErrorKind::NotFound) {
      return;
    }
    if let Some(directory) = checkout.parent() {
      let _ = fs::remove_file(directory.join(RECORD));
      let _ = fs::remove_dir(directory);
    }
  }
}
