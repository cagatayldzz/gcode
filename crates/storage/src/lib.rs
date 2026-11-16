//! Persistent state for Vekro, stored under `~/Library/Application Support/Vekro`.

mod paths;
mod preferences;
mod projects;

pub use paths::data_dir;
pub use preferences::{Preferences, ThemeChoice};
pub use projects::{Project, ProjectList};
