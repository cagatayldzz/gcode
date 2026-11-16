//! Persistent state for Vekro, stored under `~/Library/Application Support/Vekro`.

mod paths;
mod projects;

pub use paths::data_dir;
pub use projects::{Project, ProjectList};
