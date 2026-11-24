//! A small language server client: it runs the language server a project provides, keeps it told about the open files and
//! reports the problems it finds (errors and warnings), the way an editor's status bar and Problems list show them.
//!
//! It knows nothing about the interface: problems come out of a channel.

mod server;
mod uri;

pub use server::{find_server, language_id, parse_locations, Server, Session, Update};
pub use uri::{file_path, file_uri};

/// How serious a problem is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
  Error,
  Warning,
  Info,
  Hint,
}

/// A problem a language server found in a file. Lines and columns start at 0; a column counts UTF-16 units, as in the protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
  pub line: u32,
  pub column: u32,
  pub end_line: u32,
  pub end_column: u32,
  pub severity: Severity,
  pub message: String,
  pub code: Option<String>,
  pub source: Option<String>,
}

/// A place in a file, as a definition request answers it. Lines and columns start at 0, columns count UTF-16 units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
  pub path: std::path::PathBuf,
  pub line: u32,
  pub column: u32,
  pub end_line: u32,
  pub end_column: u32,
  /// The text the request was made on, when the server says how far it reaches: `(line, column, end line, end column)`.
  pub origin: Option<(u32, u32, u32, u32)>,
}
