use crate::{http, manifest::parse_manifest, Available, ExtensionError, Manifest};
use sha2::{Digest, Sha256};
use std::{
  fs,
  path::{Path, PathBuf},
};

const MANIFEST_NAME: &str = "manifest.json";

/// The file in an extension's folder whose presence means it is disabled.
const DISABLED_MARK: &str = "disabled";

/// An installed extension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
  pub manifest: Manifest,
  /// Whether it is switched on. A disabled extension stays installed but does nothing, until it is enabled again.
  pub enabled: bool,
}

/// Where the installed extensions are kept: one folder each, with the manifest in it.
fn extensions_dir() -> PathBuf {
  ws_storage::data_dir().join("extensions")
}

/// The extensions that are installed.
pub fn installed() -> Vec<Installed> {
  installed_in(&extensions_dir())
}

/// The extensions installed in `dir`, by name. A folder with no readable manifest is not an extension and is passed over.
pub fn installed_in(dir: &Path) -> Vec<Installed> {
  let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
  let mut found: Vec<Installed> = entries
    .flatten()
    .filter_map(|entry| {
      let text = fs::read_to_string(entry.path().join(MANIFEST_NAME)).ok()?;
      let manifest = parse_manifest(&text).ok()?;
      // The folder is named by the id; one that is not was not made here.
      let enabled = !entry.path().join(DISABLED_MARK).exists();
      (entry.file_name().to_str() == Some(manifest.id.as_str())).then_some(Installed { manifest, enabled })
    })
    .collect();
  found.sort_by_key(|extension| extension.manifest.name.to_lowercase());
  found
}

/// Downloads the package of `available`, checks it and installs it.
pub fn install(available: &Available) -> Result<Installed, ExtensionError> {
  if available.draft {
    return Err(ExtensionError::new(format!("{} is not available yet.", available.name)));
  }
  let (status, body) = ws_activity::timed(
    "http",
    || format!("GET {} (install an extension)", available.package_url),
    || http::get(&available.package_url),
    |result| match result {
      Ok((status, _)) => format!("status {status}"),
      Err(error) => format!("failed: {error}"),
    },
  )?;
  if status != 200 {
    return Err(ExtensionError::new(format!("Could not download {} (status {status}).", available.name)));
  }
  install_from(&extensions_dir(), available, &body)
}

/// Installs the downloaded manifest `text` of `available` in `dir`: the text must be what the index says (its SHA-256), and say it is
/// the extension the index named. Written as a whole or not at all.
pub fn install_from(dir: &Path, available: &Available, text: &str) -> Result<Installed, ExtensionError> {
  if available.draft {
    return Err(ExtensionError::new(format!("{} is not available yet.", available.name)));
  }
  if hex(&Sha256::digest(text.as_bytes())) != available.sha256 {
    return Err(ExtensionError::new("The download does not match its published checksum."));
  }
  let manifest = parse_manifest(text)?;
  if manifest.id != available.id {
    return Err(ExtensionError::new("The download is not the extension that was asked for."));
  }
  let folder = dir.join(&manifest.id);
  let write = || -> std::io::Result<()> {
    fs::create_dir_all(&folder)?;
    let temporary = folder.join(format!("{MANIFEST_NAME}.part"));
    fs::write(&temporary, text)?;
    fs::rename(temporary, folder.join(MANIFEST_NAME))
  };
  write().map_err(|error| ExtensionError::new(format!("Could not save {}: {error}", manifest.name)))?;
  // A fresh install, or an update, is switched on.
  let _ = fs::remove_file(folder.join(DISABLED_MARK));
  Ok(Installed { manifest, enabled: true })
}

/// Switches an installed extension on or off.
pub fn set_enabled(id: &str, enabled: bool) -> Result<(), ExtensionError> {
  set_enabled_in(&extensions_dir(), id, enabled)
}

/// Switches the extension `id` of `dir` on or off; the manifest stays where it is.
pub fn set_enabled_in(dir: &Path, id: &str, enabled: bool) -> Result<(), ExtensionError> {
  if !crate::index::is_plain_id(id) || !dir.join(id).join(MANIFEST_NAME).is_file() {
    return Err(ExtensionError::new("That extension is not installed."));
  }
  let mark = dir.join(id).join(DISABLED_MARK);
  let result = if enabled {
    fs::remove_file(&mark).or_else(|error| if error.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(error) })
  } else {
    fs::write(&mark, "")
  };
  result.map_err(|error| ExtensionError::new(format!("Could not change the extension: {error}")))
}

/// Removes an installed extension.
pub fn uninstall(id: &str) -> Result<(), ExtensionError> {
  uninstall_in(&extensions_dir(), id)
}

/// Removes the extension `id` from `dir`. An id that is not plain names no folder of ours.
pub fn uninstall_in(dir: &Path, id: &str) -> Result<(), ExtensionError> {
  if !crate::index::is_plain_id(id) {
    return Err(ExtensionError::new("That is not an extension."));
  }
  match fs::remove_dir_all(dir.join(id)) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(ExtensionError::new(format!("Could not remove the extension: {error}"))),
  }
}

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
