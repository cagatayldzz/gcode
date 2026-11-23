//! Persistent state for the app, stored under `~/Library/Application Support/<name>`.

mod files;
mod paths;
mod preferences;
mod profiles;
mod projects;
mod window;

pub use files::{read_json, write_atomic, write_json};
pub use paths::{data_dir, index_dir, migrate_legacy_data_dir};
pub use preferences::{ActivityBarLayout, Preferences, ThemeChoice, DEFAULT_CODE_FONT_SIZE, DEFAULT_EXPLORER_FONT_SIZE, FONT_SIZES};
pub use profiles::{delete_profile_files, Profile, ProfileList, DEFAULT_PROFILE};
pub use projects::{Category, Project, ProjectList};
pub use window::{WindowMode, WindowPlacement};
