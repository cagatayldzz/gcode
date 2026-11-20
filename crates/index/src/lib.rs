//! What Vekro works out about a project ahead of time, so that opening a project or a file only has to read the result:
//!
//! - [`ProjectIndex`]: every file the project's Git ignore rules let through, kept on disk and checked with a few `stat` calls
//!   when the project is opened again;
//! - [`highlight`]: the colors of `.env` and `.gitignore` files, worked out once for the whole text as runs that cover it.
//!
//! The crate knows nothing of the interface: roles stand for colors, and the interface maps them to its theme.

pub mod highlight;
mod index;

pub use index::{FileEntry, IndexInfo, Opened, ProjectIndex};
