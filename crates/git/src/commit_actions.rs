//! What the History list offers on a commit (revert, cherry-pick, a branch from it), and the ignore rules of the Changes list.

use git2::{Repository, RepositoryState};
use std::{
  path::Path,
  process::{Command, Output, Stdio},
};

fn message(error: git2::Error) -> String {
  error.message().to_string()
}

fn git(path: &Path, arguments: &[&str]) -> Result<Output, String> {
  ws_activity::timed(
    "git",
    || crate::describe_git(path, arguments),
    || {
      let mut command = Command::new("git");
      crate::shell_path::apply(&mut command);
      command
        .arg("-C")
        .arg(path)
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not run git: {error}"))
    },
    crate::describe_output,
  )
}

fn id_is_safe(id: &str) -> bool {
  !id.is_empty() && id.chars().all(|character| character.is_ascii_hexdigit())
}

/// The files a stopped cherry-pick or revert could not combine, from git's output.
fn conflicted_files(output: &Output) -> Vec<String> {
  let text = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
  text.lines().filter_map(|line| line.split_once("Merge conflict in ").map(|(_, file)| file.trim().to_string())).collect()
}

fn failure(output: &Output, fallback: &str) -> String {
  let text = format!("{}\n{}", String::from_utf8_lossy(&output.stderr), String::from_utf8_lossy(&output.stdout));
  text.lines().rev().find(|line| !line.trim().is_empty()).map(|line| line.trim().to_string()).unwrap_or_else(|| fallback.to_string())
}

/// Stops whatever sequence a failed command left half done, so the files are as they were.
fn stop(path: &Path, command: &str) {
  let _ = git(path, &[command, "--abort"]);
}

fn require_clean(path: &Path, action: &str) -> Result<Repository, String> {
  let repository = Repository::discover(path).map_err(message)?;
  if repository.state() != RepositoryState::Clean {
    return Err("Finish or abort the current merge first.".into());
  }
  if super::has_changes(path) {
    return Err(format!("Commit or discard your changes before {action}."));
  }
  Ok(repository)
}

/// Adds a new commit that undoes the changes of `id`, like `git revert`. A conflict stops it and leaves nothing changed.
pub fn revert_commit(path: &Path, id: &str) -> Result<String, String> {
  if !id_is_safe(id) {
    return Err("This is not a commit.".into());
  }
  let repository = require_clean(path, "reverting a commit")?;
  let commit = repository.find_commit(git2::Oid::from_str(id).map_err(message)?).map_err(message)?;
  let mut arguments = vec!["revert", "--no-edit"];
  // A merge commit is reverted against its first parent, the branch it was merged into.
  if commit.parent_count() > 1 {
    arguments.extend(["-m", "1"]);
  }
  arguments.push(id);
  let output = git(path, &arguments)?;
  if output.status.success() {
    return Ok(repository.head().map_err(message)?.peel_to_commit().map_err(message)?.id().to_string().chars().take(7).collect());
  }
  let files = conflicted_files(&output);
  stop(path, "revert");
  if files.is_empty() {
    Err(failure(&output, "git could not revert the commit."))
  } else {
    Err(format!("Reverting conflicts in {}. Nothing was changed.", files.join(", ")))
  }
}

/// Copies the changes of commit `id` onto the branch `branch` as a new commit there, like `git cherry-pick`, and leaves that branch
/// checked out. A conflict stops it and puts the repository back on the branch it was on, unchanged.
pub fn cherry_pick(path: &Path, id: &str, branch: &str) -> Result<String, String> {
  if !id_is_safe(id) {
    return Err("This is not a commit.".into());
  }
  let repository = require_clean(path, "cherry-picking")?;
  let commit = repository.find_commit(git2::Oid::from_str(id).map_err(message)?).map_err(message)?;
  if commit.parent_count() != 1 {
    return Err("Only a commit with one parent can be cherry-picked.".into());
  }
  let before = repository.head().ok().and_then(|head| head.shorthand().ok().map(str::to_string)).unwrap_or_default();
  if before != branch {
    super::switch_branch(path, branch)?;
  }
  let output = git(path, &["cherry-pick", id])?;
  if output.status.success() {
    return Ok(repository.head().map_err(message)?.peel_to_commit().map_err(message)?.id().to_string().chars().take(7).collect());
  }
  let files = conflicted_files(&output);
  stop(path, "cherry-pick");
  if before != branch {
    let _ = super::switch_branch(path, &before);
  }
  if files.is_empty() {
    Err(failure(&output, "git could not cherry-pick the commit."))
  } else {
    Err(format!("Cherry-picking onto {branch} conflicts in {}. Nothing was changed.", files.join(", ")))
  }
}

/// Creates a branch at commit `id` and switches to it. The changes in the working tree come along when they do not collide.
pub fn branch_from_commit(path: &Path, id: &str, name: &str) -> Result<(), String> {
  let name = name.trim();
  if name.is_empty() {
    return Err("Enter a branch name.".into());
  }
  if !git2::Branch::name_is_valid(name).unwrap_or(false) {
    return Err(format!("\"{name}\" is not a valid branch name."));
  }
  let repository = Repository::discover(path).map_err(message)?;
  if repository.state() != RepositoryState::Clean {
    return Err("Finish or abort the current merge first.".into());
  }
  if repository.find_branch(name, git2::BranchType::Local).is_ok() {
    return Err(format!("A branch named {name} already exists."));
  }
  let commit = repository.find_commit(git2::Oid::from_str(id).map_err(message)?).map_err(message)?;
  repository.branch(name, &commit, false).map_err(message)?;
  let mut checkout = git2::build::CheckoutBuilder::new();
  checkout.safe();
  if let Err(error) = repository.checkout_tree(commit.as_object(), Some(&mut checkout)) {
    if let Ok(mut branch) = repository.find_branch(name, git2::BranchType::Local) {
      let _ = branch.delete();
    }
    return Err(if error.code() == git2::ErrorCode::Conflict {
      format!("Your changes would be overwritten by switching to {name}. Commit or discard them first.")
    } else {
      message(error)
    });
  }
  repository.set_head(&format!("refs/heads/{name}")).map_err(message)
}

/// Adds lines to the `.gitignore` of the repository (made if there is none), skipping the ones it already has. Files Git already
/// tracks stay tracked, as with any ignore rule.
pub fn add_to_gitignore(path: &Path, patterns: &[String]) -> Result<(), String> {
  let repository = Repository::discover(path).map_err(message)?;
  let file = repository.workdir().ok_or("The repository has no working folder.")?.join(".gitignore");
  let current = std::fs::read_to_string(&file).unwrap_or_default();
  let have: std::collections::HashSet<&str> = current.lines().map(str::trim).collect();
  let mut added = String::new();
  for pattern in patterns {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.contains(['\n', '\r']) {
      return Err("This is not something to ignore.".into());
    }
    if !have.contains(pattern) && !added.lines().any(|line| line == pattern) {
      added.push_str(pattern);
      added.push('\n');
    }
  }
  if added.is_empty() {
    return Ok(());
  }
  let mut text = current.clone();
  if !text.is_empty() && !text.ends_with('\n') {
    text.push('\n');
  }
  text.push_str(&added);
  std::fs::write(&file, text).map_err(|error| format!("Could not write .gitignore: {error}"))
}
