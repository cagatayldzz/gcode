//! Extensions: optional features that are not installed until the user installs them. An extension is a package published in this
//! repository: a manifest that says what it is and what it contributes, listed in an index with the SHA-256 of the manifest. The
//! package carries no code (the app is compiled; nothing is loaded at run time): it switches on a feature that is in the app and
//! chooses among what that feature offers. Never depends on the interface.

mod http;
mod index;
mod manifest;
mod store;

pub use index::{fetch_index, index_url, parse_index, repository_slug, Available};
pub use manifest::{parse_manifest, Manifest};
pub use store::{install, install_from, installed, installed_in, set_enabled, set_enabled_in, uninstall, uninstall_in, Installed};

use std::fmt;

/// A failed step with a message that is safe to show to the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionError(String);

impl ExtensionError {
  pub(crate) fn new(message: impl Into<String>) -> Self {
    Self(message.into())
  }
}

impl fmt::Display for ExtensionError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl std::error::Error for ExtensionError {}

/// The id of the extension that provides the AI agents (Codex and Claude sessions).
pub const AI_AGENT: &str = "ai-agent";
