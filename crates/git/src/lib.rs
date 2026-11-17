//! Git access for Vekro. Never depends on the interface.

use git2::{build::CheckoutBuilder, DiffFormat, DiffOptions, Repository, Status, StatusOptions};
use std::path::{Path, PathBuf};

/// How a file differs from the last commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
  New,
  Modified,
  Deleted,
  Renamed,
  Conflicted,
}

impl ChangeKind {
  /// The one-letter mark shown beside the file.
  pub fn mark(self) -> &'static str {
    match self {
      Self::New => "A",
      Self::Modified => "M",
      Self::Deleted => "D",
      Self::Renamed => "R",
      Self::Conflicted => "!",
    }
  }
}

/// A changed file, with its path relative to the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
  pub path: PathBuf,
  pub kind: ChangeKind,
}

/// What the sidebar shows about a project's repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
  /// The checked-out branch, or a short commit id when HEAD is detached.
  pub branch: String,
  /// Files with changes in the working tree or the index, including untracked ones, sorted by path.
  pub changes: Vec<Change>,
  /// Where the branch stands against its remote; `None` when the repository has no remote.
  pub sync: Option<Sync>,
}

/// How the checked-out branch relates to its remote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sync {
  /// The remote the branch follows, or the first remote while the branch has no upstream yet (`origin` when there is one).
  pub remote: String,
  /// The remote's address.
  pub url: String,
  /// The branch has an upstream branch on the remote.
  pub upstream: bool,
  /// Commits only here, to push.
  pub ahead: usize,
  /// Commits only on the remote, to pull (as of the last fetch).
  pub behind: usize,
  /// When the remote was last fetched, from the time of `FETCH_HEAD`; `None` if never.
  pub last_fetched: Option<std::time::SystemTime>,
}

fn sync_status(repository: &Repository) -> Option<Sync> {
  let remotes = repository.remotes().ok()?;
  let head = repository.head().ok().filter(|head| head.is_branch());
  let name = head.as_ref().and_then(|head| head.shorthand()).map(str::to_string);
  let upstream =
    name.as_deref().and_then(|name| repository.find_branch(name, git2::BranchType::Local).ok()).and_then(|branch| branch.upstream().ok());
  let followed = head
    .as_ref()
    .and_then(|head| head.name())
    .and_then(|name| repository.branch_upstream_remote(name).ok())
    .and_then(|remote| remote.as_str().map(str::to_string));
  let remote = match followed {
    Some(remote) => remote,
    None => remotes.iter().flatten().find(|remote| *remote == "origin").or_else(|| remotes.iter().flatten().next())?.to_string(),
  };
  let (ahead, behind) = match (head.as_ref().and_then(|head| head.target()), upstream.as_ref().and_then(|upstream| upstream.get().target())) {
    (Some(local), Some(remote)) => repository.graph_ahead_behind(local, remote).unwrap_or((0, 0)),
    _ => (0, 0),
  };
  let last_fetched = std::fs::metadata(repository.path().join("FETCH_HEAD")).and_then(|meta| meta.modified()).ok();
  let url = repository.find_remote(&remote).ok().and_then(|found| found.url().map(str::to_string)).unwrap_or_default();
  Some(Sync { remote, url, upstream: upstream.is_some(), ahead, behind, last_fetched })
}

impl Summary {
  pub fn changed(&self) -> usize {
    self.changes.len()
  }
}

/// Reads the repository containing `path`; `None` when it is not inside one.
pub fn summary(path: &Path) -> Option<Summary> {
  let repository = Repository::discover(path).ok()?;
  let branch = match repository.head() {
    Ok(head) if head.is_branch() => head.shorthand().unwrap_or("HEAD").to_string(),
    Ok(head) => head.target().map(|id| id.to_string().chars().take(7).collect()).unwrap_or_else(|| "HEAD".into()),
    // A repository without commits still names its branch.
    Err(_) => {
      repository.find_reference("HEAD").ok().and_then(|head| head.symbolic_target().map(|name| name.trim_start_matches("refs/heads/").to_string()))?
    }
  };
  let mut options = StatusOptions::new();
  options.include_untracked(true).recurse_untracked_dirs(true);
  let mut changes: Vec<Change> = repository
    .statuses(Some(&mut options))
    .map(|statuses| {
      statuses
        .iter()
        .filter_map(|entry| {
          let status = entry.status();
          let kind = if status.contains(Status::CONFLICTED) {
            ChangeKind::Conflicted
          } else if status.intersects(Status::INDEX_NEW | Status::WT_NEW) {
            ChangeKind::New
          } else if status.intersects(Status::INDEX_DELETED | Status::WT_DELETED) {
            ChangeKind::Deleted
          } else if status.intersects(Status::INDEX_RENAMED | Status::WT_RENAMED) {
            ChangeKind::Renamed
          } else if status.is_empty() || status == Status::IGNORED {
            return None;
          } else {
            ChangeKind::Modified
          };
          Some(Change { path: PathBuf::from(entry.path()?), kind })
        })
        .collect()
    })
    .unwrap_or_default();
  changes.sort_by(|a, b| a.path.cmp(&b.path));
  let sync = sync_status(&repository);
  Some(Summary { branch, changes, sync })
}

/// The folder name `git clone` would pick for `url`: `https://github.com/a/vekro.git` gives `vekro`.
pub fn repository_name(url: &str) -> Option<String> {
  let trimmed = url.trim().trim_end_matches('/');
  let last = trimmed.rsplit(['/', ':']).next()?;
  let name = last.strip_suffix(".git").unwrap_or(last);
  (!name.is_empty()).then(|| name.to_string())
}

/// Creates the folder `parent/name` (it must not exist yet) and starts a repository on `main` in it.
pub fn create_project(parent: &Path, name: &str) -> Result<std::path::PathBuf, String> {
  let name = name.trim();
  if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
    return Err("Enter a project name without slashes.".into());
  }
  let path = parent.join(name);
  if path.exists() {
    return Err(format!("{} already exists.", path.display()));
  }
  std::fs::create_dir_all(&path).map_err(|error| format!("Could not create {}: {error}", path.display()))?;
  Repository::init_opts(&path, git2::RepositoryInitOptions::new().initial_head("main")).map_err(|error| error.message().to_string())?;
  Ok(path)
}

/// Clones `url` into `parent/<repository name>` with the `git` program, so the user's own SSH keys and
/// credential helpers apply. It never asks for input: a repository that needs a login fails with git's message.
pub fn clone(url: &str, parent: &Path, credentials: Option<&Credentials>) -> Result<std::path::PathBuf, String> {
  let url = url.trim();
  let name = repository_name(url).ok_or("Enter a repository URL.")?;
  let path = parent.join(&name);
  if path.exists() {
    return Err(format!("{} already exists.", path.display()));
  }
  let mut command = std::process::Command::new("git");
  credential_options(&mut command, credentials);
  let output = command
    .arg("clone")
    .arg("--")
    .arg(url)
    .arg(&path)
    .env("GIT_TERMINAL_PROMPT", "0")
    .output()
    .map_err(|error| format!("Could not run git: {error}"))?;
  if output.status.success() {
    Ok(path)
  } else {
    let message = String::from_utf8_lossy(&output.stderr);
    Err(message.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("git clone failed.").trim().to_string())
  }
}

/// The unified diff of one file against the last commit (staged and unstaged changes together, untracked
/// files as new). `file` is relative to the repository.
pub fn file_diff(path: &Path, file: &Path) -> Result<String, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let head = repository.head().ok().and_then(|head| head.peel_to_tree().ok());
  let mut options = DiffOptions::new();
  options.pathspec(file).include_untracked(true).show_untracked_content(true).recurse_untracked_dirs(true);
  let diff = repository.diff_tree_to_workdir_with_index(head.as_ref(), Some(&mut options)).map_err(|error| error.message().to_string())?;
  let mut patch = String::new();
  diff
    .print(DiffFormat::Patch, |_, _, line| {
      if matches!(line.origin(), '+' | '-' | ' ') {
        patch.push(line.origin());
      }
      patch.push_str(&String::from_utf8_lossy(line.content()));
      true
    })
    .map_err(|error| error.message().to_string())?;
  Ok(patch)
}

/// The absolute path of a file given relative to the repository containing `path`.
pub fn workdir_file(path: &Path, file: &Path) -> Option<PathBuf> {
  Repository::discover(path).ok()?.workdir().map(|workdir| workdir.join(file))
}

/// Throws away every change to one file (staged and unstaged) and cannot be undone. A file that is in the last commit
/// goes back to its committed content, also when it was deleted; a file that is not (a new one) is removed from the index
/// and from the disk. `file` is relative to the repository.
pub fn discard_changes(path: &Path, file: &Path) -> Result<(), String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let workdir = repository.workdir().ok_or("The repository has no working folder.")?.to_path_buf();
  let committed = repository.head().ok().and_then(|head| head.peel_to_tree().ok()).is_some_and(|tree| tree.get_path(file).is_ok());
  if committed {
    let mut checkout = CheckoutBuilder::new();
    checkout.path(file).force();
    return repository.checkout_head(Some(&mut checkout)).map_err(|error| error.message().to_string());
  }
  let mut index = repository.index().map_err(|error| error.message().to_string())?;
  if index.get_path(file, 0).is_some() {
    index.remove_path(file).map_err(|error| error.message().to_string())?;
    index.write().map_err(|error| error.message().to_string())?;
  }
  match std::fs::remove_file(workdir.join(file)) {
    Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(format!("Could not remove {}: {error}", file.display())),
    _ => Ok(()),
  }
}

/// Commits `files` (relative to the repository) with the given message, and nothing else: other changes stay as they are,
/// also those that were staged. Returns the short id of the new commit.
///
/// The commit is built from the last commit plus the chosen files, so what was staged for other files is left out of it.
/// Afterwards the index of the chosen files matches the commit, like after `git commit <files>`.
pub fn commit(path: &Path, files: &[PathBuf], summary: &str, description: &str) -> Result<String, String> {
  let summary = summary.trim();
  if summary.is_empty() {
    return Err("Write a summary for the commit.".into());
  }
  if files.is_empty() {
    return Err("Choose at least one file to commit.".into());
  }
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let workdir = repository.workdir().ok_or("The repository has no working folder.")?.to_path_buf();
  let signature = repository.signature().map_err(|_| "Set user.name and user.email in your Git configuration first.".to_string())?;
  let parent = repository.head().ok().and_then(|head| head.peel_to_commit().ok());

  // A copy of the last commit's tree in memory, with the chosen files applied.
  let mut tree_index = git2::Index::new().map_err(|error| error.message().to_string())?;
  if let Some(parent) = &parent {
    tree_index.read_tree(&parent.tree().map_err(|error| error.message().to_string())?).map_err(|error| error.message().to_string())?;
  }
  let mut real_index = repository.index().map_err(|error| error.message().to_string())?;
  for file in files {
    if workdir.join(file).exists() {
      real_index.add_path(file).map_err(|error| error.message().to_string())?;
      let entry = real_index.get_path(file, 0).ok_or_else(|| format!("Could not add {}.", file.display()))?;
      tree_index.add(&entry).map_err(|error| error.message().to_string())?;
    } else {
      // A deleted file: it leaves both the commit and the index.
      let _ = tree_index.remove_path(file);
      let _ = real_index.remove_path(file);
    }
  }
  let tree_id = tree_index.write_tree_to(&repository).map_err(|error| error.message().to_string())?;
  let tree = repository.find_tree(tree_id).map_err(|error| error.message().to_string())?;
  if parent.as_ref().is_some_and(|parent| parent.tree_id() == tree_id) {
    return Err("The chosen files have no changes to commit.".into());
  }
  let message = match description.trim() {
    "" => summary.to_string(),
    description => format!("{summary}\n\n{description}"),
  };
  let parents: Vec<&git2::Commit> = parent.iter().collect();
  let id = repository.commit(Some("HEAD"), &signature, &signature, &message, &tree, &parents).map_err(|error| error.message().to_string())?;
  real_index.write().map_err(|error| error.message().to_string())?;
  Ok(id.to_string().chars().take(7).collect())
}

/// The local branches of the repository containing `path`, sorted by name (the current one is not moved to the front).
pub fn branches(path: &Path) -> Result<Vec<String>, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let mut names: Vec<String> = repository
    .branches(Some(git2::BranchType::Local))
    .map_err(|error| error.message().to_string())?
    .filter_map(Result::ok)
    .filter_map(|(branch, _)| branch.name().ok().flatten().map(str::to_string))
    .collect();
  names.sort_by_key(|name| name.to_lowercase());
  Ok(names)
}

/// Creates a branch at the current commit and switches to it, like `git switch -c`. The changes in the working tree come along.
/// In a repository without commits the branch just becomes the name of the unborn one.
pub fn create_branch(path: &Path, name: &str) -> Result<(), String> {
  let name = name.trim();
  if name.is_empty() {
    return Err("Enter a branch name.".into());
  }
  if !git2::Branch::name_is_valid(name).unwrap_or(false) {
    return Err(format!("\"{name}\" is not a valid branch name."));
  }
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  if repository.find_branch(name, git2::BranchType::Local).is_ok() {
    return Err(format!("A branch named {name} already exists."));
  }
  match repository.head().ok().and_then(|head| head.peel_to_commit().ok()) {
    Some(commit) => {
      repository.branch(name, &commit, false).map_err(|error| error.message().to_string())?;
    }
    None => return repository.set_head(&format!("refs/heads/{name}")).map_err(|error| error.message().to_string()),
  }
  repository.set_head(&format!("refs/heads/{name}")).map_err(|error| error.message().to_string())
}

/// Switches to an existing local branch, like `git switch`. Changes that the switch would overwrite stop it: nothing is
/// forced, nothing is lost.
pub fn switch_branch(path: &Path, name: &str) -> Result<(), String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let branch = repository.find_branch(name, git2::BranchType::Local).map_err(|_| format!("There is no branch named {name}."))?;
  let commit = branch.get().peel_to_commit().map_err(|error| error.message().to_string())?;
  let mut checkout = CheckoutBuilder::new();
  checkout.safe();
  repository.checkout_tree(commit.as_object(), Some(&mut checkout)).map_err(|error| {
    if error.code() == git2::ErrorCode::Conflict {
      format!("Your changes would be overwritten by switching to {name}. Commit or discard them first.")
    } else {
      error.message().to_string()
    }
  })?;
  repository.set_head(&format!("refs/heads/{name}")).map_err(|error| error.message().to_string())
}

/// The unified diff of several files against the last commit, one file after the other (see [`file_diff`]); the input for a
/// generated commit message. `files` are relative to the repository.
pub fn diff_files(path: &Path, files: &[PathBuf]) -> Result<String, String> {
  let mut diff = String::new();
  for file in files {
    diff.push_str(&file_diff(path, file)?);
    if !diff.ends_with('\n') {
      diff.push('\n');
    }
  }
  Ok(diff)
}

/// A user name and a password (an access token) for Git over HTTPS, given to `git` for one command through a credential
/// helper that reads them from the environment, so they never appear in a command line or a file.
#[derive(Clone)]
pub struct Credentials {
  pub username: String,
  pub password: String,
}

impl std::fmt::Debug for Credentials {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.debug_struct("Credentials").field("username", &self.username).field("password", &"…").finish()
  }
}

/// The credential helper `git` is given: it answers a `get` with the user name and password from the environment.
pub const CREDENTIAL_HELPER: &str =
  "!f() { test \"$1\" = get && printf 'username=%s\\npassword=%s\\n' \"$VEKRO_GIT_USERNAME\" \"$VEKRO_GIT_PASSWORD\"; }; f";

/// The options that make `git` use `credentials` for the command: the user's own helpers are replaced for this command only.
fn credential_options(command: &mut std::process::Command, credentials: Option<&Credentials>) {
  let Some(credentials) = credentials else { return };
  command
    .args(["-c", "credential.helper=", "-c"])
    .arg(format!("credential.helper={CREDENTIAL_HELPER}"))
    .env("VEKRO_GIT_USERNAME", &credentials.username)
    .env("VEKRO_GIT_PASSWORD", &credentials.password);
}

/// Runs the `git` program in `path` with `--progress` and hands each line it reports (git redraws progress lines with a carriage
/// return) to `on_progress` while it runs. It never asks for input, so the user's own credentials apply and a login prompt becomes
/// git's error message. Returns whether it succeeded and everything it wrote to stderr.
fn run_git(path: &Path, arguments: &[&str], credentials: Option<&Credentials>, mut on_progress: impl FnMut(&str)) -> Result<(bool, String), String> {
  use std::io::Read as _;
  let mut command = std::process::Command::new("git");
  command.arg("-C").arg(path);
  credential_options(&mut command, credentials);
  let mut child = command
    .args(arguments)
    .arg("--progress")
    .env("GIT_TERMINAL_PROMPT", "0")
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .map_err(|error| format!("Could not run git: {error}"))?;
  let mut stderr = child.stderr.take().ok_or("Could not read git's output.")?;
  let (mut all, mut line, mut buffer) = (Vec::new(), Vec::new(), [0u8; 512]);
  loop {
    let read = stderr.read(&mut buffer).map_err(|error| format!("Could not read git's output: {error}"))?;
    if read == 0 {
      break;
    }
    all.extend_from_slice(&buffer[..read]);
    for byte in &buffer[..read] {
      if matches!(byte, b'\r' | b'\n') {
        let text = String::from_utf8_lossy(&line).trim().to_string();
        if !text.is_empty() {
          on_progress(&text);
        }
        line.clear();
      } else {
        line.push(*byte);
      }
    }
  }
  let status = child.wait().map_err(|error| format!("Could not run git: {error}"))?;
  Ok((status.success(), String::from_utf8_lossy(&all).to_string()))
}

/// The last line git wrote, ignoring the redrawn progress states (`\r`-separated) before it.
fn last_line(text: &str) -> String {
  text.split(['\r', '\n']).rev().find(|line| !line.trim().is_empty()).unwrap_or("").trim().to_string()
}

fn finish(result: (bool, String), failure: &str) -> Result<String, String> {
  let (success, stderr) = result;
  let line = last_line(&stderr);
  match (success, line.is_empty()) {
    (true, _) => Ok(line),
    (false, true) => Err(failure.to_string()),
    (false, false) => Err(line),
  }
}

/// Pushes the current branch with the `git` program, so the user's own credentials apply; a branch without an upstream is
/// published to `origin`. `on_progress` gets git's progress lines ("Writing objects: 45% …") while it runs.
pub fn push(path: &Path, credentials: Option<&Credentials>, mut on_progress: impl FnMut(&str)) -> Result<String, String> {
  let mut result = run_git(path, &["push"], credentials, &mut on_progress)?;
  if !result.0 && result.1.contains("no upstream branch") {
    result = run_git(path, &["push", "--set-upstream", "origin", "HEAD"], credentials, &mut on_progress)?;
  }
  finish(result, "git push failed.")
}

/// Fetches the remote of the current branch (`git fetch`). Changes nothing in the working tree.
pub fn fetch(path: &Path, credentials: Option<&Credentials>, on_progress: impl FnMut(&str)) -> Result<String, String> {
  finish(run_git(path, &["fetch"], credentials, on_progress)?, "git fetch failed.")
}

/// Brings in the remote's new commits with `git pull --ff-only`: it only moves the branch forward, never creates a merge, and
/// refuses (with git's message) when the branches have diverged or local changes are in the way.
pub fn pull(path: &Path, credentials: Option<&Credentials>, on_progress: impl FnMut(&str)) -> Result<String, String> {
  finish(run_git(path, &["pull", "--ff-only"], credentials, on_progress)?, "git pull failed.")
}

/// The URL of the remote the current branch follows, else `origin`, else the first remote; `None` without remotes.
pub fn remote_url(path: &Path) -> Option<String> {
  let repository = Repository::discover(path).ok()?;
  let remotes = repository.remotes().ok()?;
  let followed = repository
    .head()
    .ok()
    .and_then(|head| head.name().map(str::to_string))
    .and_then(|name| repository.branch_upstream_remote(&name).ok())
    .and_then(|remote| remote.as_str().map(str::to_string));
  let name = followed
    .or_else(|| remotes.iter().flatten().find(|name| *name == "origin").map(str::to_string))
    .or_else(|| remotes.iter().flatten().next().map(str::to_string))?;
  let remote = repository.find_remote(&name).ok()?;
  let url = remote.url().map(str::to_string);
  url
}

/// The first line of the last commit's message.
pub fn head_summary(path: &Path) -> Option<String> {
  let repository = Repository::discover(path).ok()?;
  let commit = repository.head().ok()?.peel_to_commit().ok()?;
  commit.summary().map(str::to_string)
}

/// The folders directly inside `folder` that are Git repositories, sorted by name. Folders that cannot be read are skipped.
pub fn find_repositories(folder: &Path) -> Vec<PathBuf> {
  let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
  let mut found: Vec<PathBuf> = entries.filter_map(Result::ok).map(|entry| entry.path()).filter(|path| path.join(".git").exists()).collect();
  found.sort_by_key(|path| path.file_name().map(|name| name.to_string_lossy().to_lowercase()));
  found
}
