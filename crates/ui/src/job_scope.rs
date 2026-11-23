//! Background jobs belong to the project they started in, and the window may show another project by the time they end. These rules
//! decide where a finished job lands, what its message says and what the tabs remember, without touching the interface.

use std::{
  collections::HashMap,
  path::{Path, PathBuf},
};

/// Whether `root` is still the project the window shows, so a finished job may change what the window shows.
pub(crate) fn lands_here(active: Option<&Path>, root: &Path) -> bool {
  active == Some(root)
}

/// `message` for a job that ended in another project than the open one: the project is named, so the message is not taken for the open
/// project's. `place` is the project's name when it is not the open one, else `None`. A closing full stop stays last.
pub(crate) fn name_the_place(message: &str, place: Option<&str>) -> String {
  match place {
    None => message.to_string(),
    Some(place) => match message.strip_suffix('.') {
      Some(body) => format!("{body} ({place})."),
      None => format!("{message} ({place})"),
    },
  }
}

/// Keeps `status` for `root` in what the tabs show. Returns whether anything changed; `None` (the status could not be read) changes nothing.
pub(crate) fn remember_status<T: PartialEq>(known: &mut HashMap<PathBuf, T>, root: &Path, status: Option<T>) -> bool {
  let Some(status) = status else { return false };
  if known.get(root) == Some(&status) {
    return false;
  }
  known.insert(root.to_path_buf(), status);
  true
}

/// Whether a piece of work that belongs to `owner` is shown in the window that shows `active`.
pub(crate) fn shown_in(owner: &Path, active: Option<&Path>) -> bool {
  lands_here(active, owner)
}

/// The commit message typed for one project, put away while another project is open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Draft {
  pub summary: String,
  pub description: String,
}

impl Draft {
  pub(crate) fn is_empty(&self) -> bool {
    self.summary.is_empty() && self.description.is_empty()
  }
}

/// Drafts of the projects that are not open. A draft that is empty is not kept.
#[derive(Debug, Default)]
pub(crate) struct Drafts {
  put_away: HashMap<PathBuf, Draft>,
}

impl Drafts {
  /// Puts away what was typed for `root`.
  pub(crate) fn put_away(&mut self, root: &Path, draft: Draft) {
    if draft.is_empty() {
      self.put_away.remove(root);
    } else {
      self.put_away.insert(root.to_path_buf(), draft);
    }
  }

  /// Takes the draft of `root` back (empty when there is none).
  pub(crate) fn take(&mut self, root: &Path) -> Draft {
    self.put_away.remove(root).unwrap_or_default()
  }

  /// Replaces the draft of a project that is not open, for example with the message of a commit that was taken back there.
  pub(crate) fn set(&mut self, root: &Path, draft: Draft) {
    self.put_away(root, draft);
  }

  /// Throws the draft of `root` away, for example after the commit was made there.
  pub(crate) fn clear(&mut self, root: &Path) {
    self.put_away.remove(root);
  }
}
