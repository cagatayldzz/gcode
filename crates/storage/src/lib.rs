//! Persistent state for Vekro, stored under `~/Library/Application Support/Vekro`.

mod paths;
mod preferences;
mod projects;

pub use paths::data_dir;
pub use preferences::{Preferences, ThemeChoice, CODE_FONT_SIZES, DEFAULT_CODE_FONT_SIZE};
pub use projects::{Category, Project, ProjectList};
