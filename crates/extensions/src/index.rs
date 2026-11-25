use crate::{http, ExtensionError};

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// The branch of the repository that the index and the packages are read from.
const BRANCH: &str = "main";

/// The folder of the repository that holds the index and the packages.
const FOLDER: &str = "extensions";

/// The most extensions one index lists; a longer list is cut.
const MAX_LISTED: usize = 200;

/// An extension the index offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Available {
  pub id: String,
  pub name: String,
  pub description: String,
  pub publisher: String,
  pub version: String,
  /// The name of an icon the app has (`icons/…`), shown in the list.
  pub icon: String,
  /// Where the manifest is, always in this repository.
  pub package_url: String,
  /// The SHA-256 of the manifest, as the index says it must be.
  pub sha256: String,
}

/// `owner/name` of the GitHub repository that publishes the extensions.
pub fn repository_slug() -> Option<&'static str> {
  REPOSITORY.strip_prefix("https://github.com/").map(|slug| slug.trim_end_matches('/').trim_end_matches(".git")).filter(|s| s.contains('/'))
}

fn raw_prefix(slug: &str) -> String {
  format!("https://raw.githubusercontent.com/{slug}/{BRANCH}/{FOLDER}/")
}

/// Where the index is.
pub fn index_url() -> Option<String> {
  repository_slug().map(|slug| format!("{}index.json", raw_prefix(slug)))
}

/// Reads the index of the registry.
pub fn fetch_index() -> Result<Vec<Available>, ExtensionError> {
  let slug = repository_slug().ok_or_else(|| ExtensionError::new("The extension repository is not configured."))?;
  let url = index_url().ok_or_else(|| ExtensionError::new("The extension repository is not configured."))?;
  let (status, body) = ws_activity::timed(
    "http",
    || "GET extensions/index.json (the list of extensions)".into(),
    || http::get(&url),
    |result| match result {
      Ok((status, _)) => format!("status {status}"),
      Err(error) => format!("failed: {error}"),
    },
  )?;
  match status {
    200 => parse_index(&body, slug),
    404 => Ok(Vec::new()),
    other => Err(ExtensionError::new(format!("The extension registry answered with status {other}."))),
  }
}

/// Reads an index (`{"extensions": [{"id", "name", "description", "publisher", "version", "icon", "package", "sha256"}]}`). `package`
/// is a path inside the extensions folder of the repository `slug`; an entry whose id, path or hash is not plain is left out, so a
/// tampered index cannot point the app at another place or another file.
pub fn parse_index(json: &str, slug: &str) -> Result<Vec<Available>, ExtensionError> {
  let value: serde_json::Value = serde_json::from_str(json).map_err(|_| ExtensionError::new("Could not read the list of extensions."))?;
  let entries = value["extensions"].as_array().ok_or_else(|| ExtensionError::new("The list of extensions is missing its entries."))?;
  let prefix = raw_prefix(slug);
  let text = |entry: &serde_json::Value, key: &str| entry[key].as_str().unwrap_or_default().trim().to_string();
  Ok(
    entries
      .iter()
      .filter_map(|entry| {
        let (id, package, sha256) = (text(entry, "id"), text(entry, "package"), text(entry, "sha256").to_ascii_lowercase());
        let plain =
          |part: &str| !part.is_empty() && part != ".." && part != "." && part.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
        let valid = is_plain_id(&id) && package.split('/').all(plain) && sha256.len() == 64 && sha256.chars().all(|c| c.is_ascii_hexdigit());
        valid.then(|| Available {
          name: Some(text(entry, "name")).filter(|name| !name.is_empty()).unwrap_or_else(|| id.clone()),
          description: text(entry, "description"),
          publisher: text(entry, "publisher"),
          version: text(entry, "version"),
          icon: Some(text(entry, "icon"))
            .filter(|icon| icon.starts_with("icons/") && !icon.contains(".."))
            .unwrap_or_else(|| "icons/package.svg".into()),
          package_url: format!("{prefix}{package}"),
          sha256,
          id,
        })
      })
      .take(MAX_LISTED)
      .collect(),
  )
}

/// An extension id: lower-case letters, digits and dashes, so that it is safe as a folder name.
pub(crate) fn is_plain_id(id: &str) -> bool {
  !id.is_empty() && id.len() <= 64 && !id.starts_with('-') && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
