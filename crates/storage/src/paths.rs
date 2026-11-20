use std::path::PathBuf;

/// The application data directory (`$VEKRO_DATA_DIR` overrides it, mainly for tests).
pub fn data_dir() -> PathBuf {
  if let Some(dir) = std::env::var_os("VEKRO_DATA_DIR") {
    return PathBuf::from(dir);
  }
  let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
  home.join("Library").join("Application Support").join("Vekro")
}

/// Where the project indexes are kept (`index/` in the data directory). They are caches: deleting the folder only costs a new walk
/// of each project.
pub fn index_dir() -> PathBuf {
  data_dir().join("index")
}
