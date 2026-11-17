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
  patch_text(&diff)
}

/// A diff as the text of a unified patch.
fn patch_text(diff: &git2::Diff) -> Result<String, String> {
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

/// What the message of a failed push says when the server accepted the login but refused commits that change workflow files
/// (GitHub does so unless the token may change workflows): the interface looks for it to mark the account as having a problem.
pub const WORKFLOWS_REFUSED: &str = "Workflows: Read and write";

/// Whether a line is git's progress report ("Resolving deltas:  43% (20/46)", "Counting objects: …"), not something to tell the user.
fn is_progress(line: &str) -> bool {
  const STEPS: [&str; 7] =
    ["Resolving deltas", "Counting objects", "Compressing objects", "Enumerating objects", "Writing objects", "Delta compression", "Total "];
  STEPS.iter().any(|step| line.starts_with(step)) || line.contains("% (")
}

/// Why a git command failed, in a sentence. git ends its output with a line such as "failed to push some refs", which says nothing;
/// the reason is in the line about the rejected branch or in what the remote said.
fn failure_reason(stderr: &str, failure: &str) -> String {
  let lines: Vec<&str> = stderr.split(['\r', '\n']).map(str::trim).filter(|line| !line.is_empty()).collect();
  let mut message = match lines.iter().find(|line| line.contains("[remote rejected]") || line.contains("[rejected]")) {
    Some(rejected) => {
      // The remote's own words, when it said any, come with the rejection.
      let mut said: Vec<&str> =
        lines.iter().filter_map(|line| line.strip_prefix("remote:")).map(str::trim).filter(|line| !line.is_empty() && !is_progress(line)).collect();
      said.dedup();
      let rejected = rejected.trim_start_matches('!').trim();
      if said.is_empty() {
        rejected.to_string()
      } else {
        format!("{rejected}: {}", said.join(" "))
      }
    }
    None => match last_line(stderr) {
      line if line.is_empty() => failure.to_string(),
      line => line,
    },
  };
  let lowered = stderr.to_lowercase();
  if lowered.contains("workflow") && (lowered.contains("scope") || lowered.contains("permission")) {
    message.push_str(&format!(
      " The token needs the {WORKFLOWS_REFUSED} permission (a classic token, the `workflow` scope), because the commits change files in .github/workflows."
    ));
  }
  message
}

fn finish(result: (bool, String), failure: &str) -> Result<String, String> {
  let (success, stderr) = result;
  if success {
    return Ok(last_line(&stderr));
  }
  Err(failure_reason(&stderr, failure))
}

/// What the message of a failed push, fetch or pull says when the server turned down the connected account's token (and the user's own
/// login could not be used either): the interface looks for it to mark the account as having a problem.
pub const TOKEN_REFUSED: &str = "refused the token of the connected account";

/// Whether git's message says the server turned the login down (rather than, say, a network failure).
fn login_refused(message: &str) -> bool {
  ["403", "401", "Authentication failed", "denied", "could not read Username", "Permission to"].iter().any(|sign| message.contains(sign))
}

/// Runs a command that talks to the remote with the connected account's token; if the server refuses that login, it is run
/// once more with the user's own Git login (SSH key, credential helper), which is what happened before an account was
/// connected. When both are refused, the message says so.
fn with_login(credentials: Option<&Credentials>, mut command: impl FnMut(Option<&Credentials>) -> Result<String, String>) -> Result<String, String> {
  match (command(credentials), credentials) {
    (Err(first), Some(_)) if login_refused(&first) => command(None).map_err(|second| {
      if login_refused(&second) {
        format!(
          "The server {TOKEN_REFUSED} ({}). Your own Git login could not be used either ({}). Give the token write access in Settings → Accounts (reconnect the account with a new token), or sign in once with `git push` in a terminal.",
          first.trim_end_matches('.'),
          second.trim_end_matches('.')
        )
      } else {
        second
      }
    }),
    (result, _) => result,
  }
}

/// Pushes the current branch with the `git` program, so the user's own credentials apply; a branch without an upstream is
/// published to `origin`. `on_progress` gets git's progress lines ("Writing objects: 45% …") while it runs. With `credentials`
/// (a connected account's token) they are tried first, and the user's own login after a refusal.
pub fn push(path: &Path, credentials: Option<&Credentials>, mut on_progress: impl FnMut(&str)) -> Result<String, String> {
  with_login(credentials, |credentials| {
    let mut result = run_git(path, &["push"], credentials, &mut on_progress)?;
    if !result.0 && result.1.contains("no upstream branch") {
      result = run_git(path, &["push", "--set-upstream", "origin", "HEAD"], credentials, &mut on_progress)?;
    }
    finish(result, "git push failed.")
  })
}

/// Fetches the remote of the current branch (`git fetch`). Changes nothing in the working tree.
pub fn fetch(path: &Path, credentials: Option<&Credentials>, mut on_progress: impl FnMut(&str)) -> Result<String, String> {
  with_login(credentials, |credentials| finish(run_git(path, &["fetch"], credentials, &mut on_progress)?, "git fetch failed."))
}

/// Brings in the remote's new commits with `git pull --ff-only`: it only moves the branch forward, never creates a merge, and
/// refuses (with git's message) when the branches have diverged or local changes are in the way.
pub fn pull(path: &Path, credentials: Option<&Credentials>, mut on_progress: impl FnMut(&str)) -> Result<String, String> {
  with_login(credentials, |credentials| finish(run_git(path, &["pull", "--ff-only"], credentials, &mut on_progress)?, "git pull failed."))
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

/// A commit in the history list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitInfo {
  /// The full id.
  pub id: String,
  /// The first seven characters of the id.
  pub short: String,
  /// The first line of the message.
  pub summary: String,
  pub author: String,
  /// When it was made, in seconds since 1970.
  pub time: i64,
  /// Not on the remote yet: the branch's upstream does not have it (or, without an upstream, no remote branch does).
  pub unpushed: bool,
  /// The tags that point at it.
  pub tags: Vec<String>,
}

/// A commit with what it changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitDetail {
  pub info: CommitInfo,
  /// The message after its first line.
  pub body: String,
  /// The files it changed, sorted by path (against its first parent; a first commit against nothing).
  pub files: Vec<Change>,
  pub additions: usize,
  pub deletions: usize,
}

fn commit_info(
  commit: &git2::Commit,
  unpushed: &std::collections::HashSet<git2::Oid>,
  tags: &std::collections::HashMap<git2::Oid, Vec<String>>,
) -> CommitInfo {
  let id = commit.id().to_string();
  CommitInfo {
    short: id.chars().take(7).collect(),
    id,
    summary: commit.summary().unwrap_or("").to_string(),
    author: commit.author().name().unwrap_or("").to_string(),
    time: commit.time().seconds(),
    unpushed: unpushed.contains(&commit.id()),
    tags: tags.get(&commit.id()).cloned().unwrap_or_default(),
  }
}

/// The last `limit` commits reachable from HEAD, newest first, with which of them are not on the remote yet and their tags.
pub fn history(path: &Path, limit: usize) -> Result<Vec<CommitInfo>, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  if repository.head().is_err() {
    return Ok(Vec::new());
  }
  // Commits the remote does not have: from HEAD without what the upstream (else every remote branch) reaches.
  let mut unpushed = std::collections::HashSet::new();
  if repository.remotes().map(|remotes| !remotes.is_empty()).unwrap_or(false) {
    let mut walk = repository.revwalk().map_err(|error| error.message().to_string())?;
    walk.push_head().map_err(|error| error.message().to_string())?;
    let upstream = repository
      .head()
      .ok()
      .and_then(|head| head.shorthand().map(str::to_string))
      .and_then(|name| repository.find_branch(&name, git2::BranchType::Local).ok())
      .and_then(|branch| branch.upstream().ok())
      .and_then(|upstream| upstream.get().target());
    match upstream {
      Some(target) => {
        let _ = walk.hide(target);
      }
      None => {
        let _ = walk.hide_glob("refs/remotes/*");
      }
    }
    unpushed.extend(walk.filter_map(Result::ok));
  }
  let mut tags: std::collections::HashMap<git2::Oid, Vec<String>> = std::collections::HashMap::new();
  let _ = repository.tag_foreach(|id, name| {
    if let Some(commit) = repository.find_object(id, None).ok().and_then(|object| object.peel_to_commit().ok()) {
      tags.entry(commit.id()).or_default().push(String::from_utf8_lossy(name).trim_start_matches("refs/tags/").to_string());
    }
    true
  });
  let mut walk = repository.revwalk().map_err(|error| error.message().to_string())?;
  walk.set_sorting(git2::Sort::TIME).map_err(|error| error.message().to_string())?;
  walk.push_head().map_err(|error| error.message().to_string())?;
  Ok(
    walk
      .filter_map(Result::ok)
      .filter_map(|id| repository.find_commit(id).ok())
      .take(limit)
      .map(|commit| commit_info(&commit, &unpushed, &tags))
      .collect(),
  )
}

fn commit_diff<'a>(repository: &'a Repository, commit: &git2::Commit, file: Option<&Path>) -> Result<git2::Diff<'a>, String> {
  let tree = commit.tree().map_err(|error| error.message().to_string())?;
  let parent = commit.parent(0).ok().and_then(|parent| parent.tree().ok());
  let mut options = DiffOptions::new();
  if let Some(file) = file {
    options.pathspec(file);
  }
  repository.diff_tree_to_tree(parent.as_ref(), Some(&tree), Some(&mut options)).map_err(|error| error.message().to_string())
}

/// What a commit says and changed.
pub fn commit_detail(path: &Path, id: &str) -> Result<CommitDetail, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let commit =
    repository.find_commit(git2::Oid::from_str(id).map_err(|error| error.message().to_string())?).map_err(|error| error.message().to_string())?;
  let diff = commit_diff(&repository, &commit, None)?;
  let mut files: Vec<Change> = diff
    .deltas()
    .filter_map(|delta| {
      let kind = match delta.status() {
        git2::Delta::Added | git2::Delta::Untracked => ChangeKind::New,
        git2::Delta::Deleted => ChangeKind::Deleted,
        git2::Delta::Renamed => ChangeKind::Renamed,
        git2::Delta::Conflicted => ChangeKind::Conflicted,
        _ => ChangeKind::Modified,
      };
      let file = if kind == ChangeKind::Deleted { delta.old_file().path() } else { delta.new_file().path() }?;
      Some(Change { path: file.to_path_buf(), kind })
    })
    .collect();
  files.sort_by(|a, b| a.path.cmp(&b.path));
  let stats = diff.stats().map_err(|error| error.message().to_string())?;
  let message = commit.message().unwrap_or("");
  let body = message.split_once('\n').map(|(_, rest)| rest.trim().to_string()).unwrap_or_default();
  Ok(CommitDetail {
    info: commit_info(&commit, &std::collections::HashSet::new(), &std::collections::HashMap::new()),
    body,
    files,
    additions: stats.insertions(),
    deletions: stats.deletions(),
  })
}

/// The unified diff of one file in a commit, against its first parent. `file` is relative to the repository.
pub fn commit_file_diff(path: &Path, id: &str, file: &Path) -> Result<String, String> {
  let repository = Repository::discover(path).map_err(|error| error.message().to_string())?;
  let commit =
    repository.find_commit(git2::Oid::from_str(id).map_err(|error| error.message().to_string())?).map_err(|error| error.message().to_string())?;
  let diff = commit_diff(&repository, &commit, Some(file))?;
  patch_text(&diff)
}

/// The folders directly inside `folder` that are Git repositories, sorted by name. Folders that cannot be read are skipped.
pub fn find_repositories(folder: &Path) -> Vec<PathBuf> {
  let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
  let mut found: Vec<PathBuf> = entries.filter_map(Result::ok).map(|entry| entry.path()).filter(|path| path.join(".git").exists()).collect();
  found.sort_by_key(|path| path.file_name().map(|name| name.to_string_lossy().to_lowercase()));
  found
}
