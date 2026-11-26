//! Persistent state for the app, stored under `~/Library/Application Support/<name>`.

mod crash_log;
mod files;
mod paths;
mod preferences;
mod profiles;
mod projects;
mod recent;
mod recovery;
mod window;

pub use crash_log::{format_report, install_panic_hook, logs_dir, write_report, KEPT_REPORTS};
pub use files::{read_json, write_atomic, write_if_current, write_json, write_new, FileVersion};
pub use paths::{data_dir, index_dir, migrate_legacy_data_dir};
pub use preferences::{ActivityBarLayout, FontChoice, Preferences, ThemeChoice, DEFAULT_CODE_FONT_SIZE, DEFAULT_EXPLORER_FONT_SIZE, FONT_SIZES};
pub use profiles::{delete_profile_files, Profile, ProfileList, DEFAULT_PROFILE};
pub use projects::{Category, Project, ProjectList};
pub use recent::{RecentEntry, RecentList};
pub use recovery::{Draft, RecoveryJournal};
pub use window::{WindowMode, WindowPlacement};
