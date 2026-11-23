use std::path::PathBuf;

/// The application data directory (`<prefix>_DATA_DIR` overrides it, mainly for tests).
pub fn data_dir() -> PathBuf {
  if let Some(dir) = ws_product::env_var("DATA_DIR") {
    return PathBuf::from(dir);
  }
  application_support().join(ws_product::NAME)
}

fn application_support() -> PathBuf {
  let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
  home.join("Library").join("Application Support")
}

/// Moves the data folder the product had under its previous name to the current one, once: when the current folder does not exist
/// yet and the old one does. Does nothing when the folder was named by the environment. Returns whether it moved anything.
pub fn migrate_legacy_data_dir() -> std::io::Result<bool> {
  if ws_product::env_var("DATA_DIR").is_some() {
    return Ok(false);
  }
  let (current, legacy) = (application_support().join(ws_product::NAME), application_support().join(ws_product::LEGACY_NAME));
  // A case-insensitive file system sees both names as one folder: there is nothing to move.
  let same = legacy.exists() && current.exists() && std::fs::canonicalize(&legacy)? == std::fs::canonicalize(&current)?;
  if same || current.exists() || !legacy.is_dir() {
    return Ok(false);
  }
  std::fs::rename(legacy, current)?;
  Ok(true)
}

/// Where the project indexes are kept (`index/` in the data directory). They are caches: deleting the folder only costs a new walk
/// of each project.
pub fn index_dir() -> PathBuf {
  data_dir().join("index")
}
