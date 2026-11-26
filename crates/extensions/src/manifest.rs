use crate::{index::is_plain_id, ExtensionError};

/// The longest a manifest is read as; the answer of the registry is already limited, this guards the files on disk.
const MAX_FIELD: usize = 400;

/// What a package says it is and contributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
  pub id: String,
  pub name: String,
  pub version: String,
  pub description: String,
  pub publisher: String,
  /// The agents the extension switches on, by id (`codex`, `claude`). Which command each one runs is decided by the app, never by
  /// the manifest: a package published online must not choose what the shell runs.
  pub agents: Vec<String>,
}

/// Reads a manifest: `{"id", "name", "version", "description", "publisher", "contributes": {"agents": ["codex", "claude"]}}`.
pub fn parse_manifest(json: &str) -> Result<Manifest, ExtensionError> {
  let invalid = || ExtensionError::new("The extension's manifest is not valid.");
  let value: serde_json::Value = serde_json::from_str(json).map_err(|_| invalid())?;
  let text = |key: &str| value[key].as_str().unwrap_or_default().trim().chars().take(MAX_FIELD).collect::<String>();
  let id = text("id");
  if !is_plain_id(&id) {
    return Err(invalid());
  }
  let agents = value["contributes"]["agents"]
    .as_array()
    .map(|agents| agents.iter().filter_map(|agent| agent.as_str()).filter(|agent| is_plain_id(agent)).map(str::to_string).collect())
    .unwrap_or_default();
  Ok(Manifest {
    name: Some(text("name")).filter(|name| !name.is_empty()).unwrap_or_else(|| id.clone()),
    id,
    version: text("version"),
    description: text("description"),
    publisher: text("publisher"),
    agents,
  })
}

/// Whether `candidate` is a higher semantic version (`major.minor.patch`) than `current`. A version that is not of that form is never
/// newer, so a malformed index cannot offer an "update".
pub fn is_newer(candidate: &str, current: &str) -> bool {
  fn parts(version: &str) -> Option<[u64; 3]> {
    let mut numbers = version.trim().trim_start_matches('v').split('.').map(|part| part.parse::<u64>().ok());
    let found = [numbers.next()??, numbers.next()??, numbers.next()??];
    numbers.next().is_none().then_some(found)
  }
  match (parts(candidate), parts(current)) {
    (Some(candidate), Some(current)) => candidate > current,
    _ => false,
  }
}
