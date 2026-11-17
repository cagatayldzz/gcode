//! Vekro's self-update: finds the latest GitHub release, downloads and verifies it (SHA-256, code signature, Gatekeeper,
//! matching developer team), then swaps the running `.app` bundle and relaunches it. Never depends on the interface.

mod install;
mod process;
mod release;

pub use install::{bundle_of, current_app_bundle, discard, install, prepare, team_identifier, Stage, Staged, SWAP_SCRIPT};
pub use release::{check, checksum_for, parse_release, repository_slug, Release};

use std::fmt;

/// A failed update step with a message that is safe to show to the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateError(String);

impl UpdateError {
  pub(crate) fn new(message: impl Into<String>) -> Self {
    Self(message.into())
  }
}

impl fmt::Display for UpdateError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl std::error::Error for UpdateError {}
