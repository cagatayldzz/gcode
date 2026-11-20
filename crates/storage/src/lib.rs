//! Persistent state for Vekro, stored under `~/Library/Application Support/Vekro`.

mod paths;
mod preferences;
mod projects;
mod window;

pub use paths::data_dir;
pub use preferences::{ActivityBarLayout, Preferences, ThemeChoice, DEFAULT_CODE_FONT_SIZE, FONT_SIZES};
pub use projects::{Category, Project, ProjectList};
pub use window::{WindowMode, WindowPlacement};
