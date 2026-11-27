//! File selection and the index transaction around Git's native commit.

use git2::{ErrorCode, Oid, Repository};
use std::{
  collections::HashSet,
  ffi::{OsStr, OsString},
  fs::{self, File, OpenOptions},
  io,
  os::unix::ffi::OsStrExt as _,
  path::{Component, Path, PathBuf},
  process::{Command, Output, Stdio},
};

/// Git committed successfully, but the caller may need to show a warning before any follow-up operation.
#[derive(Debug, PartialEq, Eq)]
pub struct CommitOutcome {
  pub id: String,
  pub warning: Option<String>,
}

/// Commits the selected working-tree files and preserves unrelated staged changes.
pub fn commit(path: &Path, files: &[PathBuf], summary: &str, description: &str) -> Result<CommitOutcome, String> {
  build_commit(path, files, summary, description, false, false)
}

/// Amends an unpublished, non-merge commit with the selected files or only a new message.
pub fn amend_commit(path: &Path, files: &[PathBuf], summary: &str, description: &str) -> Result<CommitOutcome, String> {
  build_commit(path, files, summary, description, true, false)
}

/// Commits the current index, including any changes staged from the terminal.
pub fn commit_staged(path: &Path, summary: &str, description: &str) -> Result<CommitOutcome, String> {
  build_commit(path, &[], summary, description, false, true)
}

#[derive(PartialEq, Eq)]
pub(crate) struct Head {
  branch: Option<Vec<u8>>,
  pub(crate) tip: Option<Oid>,
}

impl Head {
  pub(crate) fn read(repository: &Repository) -> Result<Self, String> {
    let reference = repository.find_reference("HEAD").map_err(message)?;
    let branch = reference.symbolic_target_bytes().map(<[u8]>::to_vec);
    let tip = match repository.head() {
      Ok(head) => Some(head.peel_to_commit().map_err(message)?.id()),
      Err(error) if error.code() == ErrorCode::UnbornBranch => None,
      Err(error) => return Err(message(error)),
    };
    Ok(Self { branch, tip })
  }
}

/// Hold Git's real index lock throughout, even when a hook fails. Git writes its own locks beside the isolated index.
struct IndexLock(PathBuf);

impl IndexLock {
  fn acquire(index: &Path) -> Result<Self, String> {
    let mut name = index.as_os_str().to_os_string();
    name.push(".lock");
    let lock = PathBuf::from(name);
    OpenOptions::new().create_new(true).write(true).open(&lock).map_err(|error| {
      if error.kind() == io::ErrorKind::AlreadyExists {
        "Git's index is locked. Finish the other Git operation and retry.".to_string()
      } else {
        format!("Could not lock Git's index: {error}")
      }
    })?;
    Ok(Self(lock))
  }
}

impl Drop for IndexLock {
  fn drop(&mut self) {
    let _ = fs::remove_file(&self.0);
  }
}

pub(crate) struct IndexTransaction {
  pub(crate) scratch: tempfile::TempDir,
  pub(crate) shadow: PathBuf,
  real: PathBuf,
  _lock: IndexLock,
}

impl IndexTransaction {
  pub(crate) fn open(repository: &Repository, workdir: &Path) -> Result<Self, String> {
    let real = repository.path().join("index");
    let lock = IndexLock::acquire(&real)?;
    let scratch = tempfile::Builder::new().prefix("index-").tempdir_in(repository.path()).map_err(|error| error.to_string())?;
    let shadow = scratch.path().join("index");
    match fs::copy(&real, &shadow) {
      Ok(_) => {}
      Err(error) if error.kind() == io::ErrorKind::NotFound => {
        run(workdir, &shadow, "read-tree", ["--empty"])?;
      }
      Err(error) => return Err(format!("Could not copy Git's index: {error}")),
    }
    Ok(Self { scratch, shadow, real, _lock: lock })
  }

  pub(crate) fn publish(&self) -> Result<(), String> {
    File::open(&self.shadow).and_then(|file| file.sync_all()).and_then(|()| fs::rename(&self.shadow, &self.real)).map_err(|error| error.to_string())
  }
}

pub(crate) fn validate_paths(files: &[PathBuf]) -> Result<(), String> {
  if files.iter().any(|path| path.as_os_str().is_empty() || path.components().any(|part| !matches!(part, Component::Normal(_)))) {
    return Err("Select files inside the repository.".into());
  }
  Ok(())
}

fn build_commit(path: &Path, files: &[PathBuf], summary: &str, description: &str, amend: bool, staged: bool) -> Result<CommitOutcome, String> {
  let summary = summary.trim();
  if summary.is_empty() {
    return Err("Write a summary for the commit.".into());
  }
  if files.is_empty() && !amend && !staged {
    return Err("Choose at least one file to commit.".into());
  }
  validate_paths(files)?;
  let repository = Repository::discover(path).map_err(message)?;
  let workdir = repository.workdir().ok_or("The repository has no working folder.")?;
  let index = IndexTransaction::open(&repository, workdir)?;
  super::operation::require_idle(&repository)?;
  let before = Head::read(&repository)?;
  if amend {
    let last = repository.find_commit(before.tip.ok_or("There is no commit to amend.")?).map_err(message)?;
    if last.parent_count() > 1 {
      return Err("A merge commit cannot be amended here.".into());
    }
    if super::on_remote(&repository, last.id()) {
      return Err("This commit is already on the remote; amending it would rewrite published history.".into());
    }
  }
  let shadow = &index.shadow;
  let existing_stage =
    if staged { Vec::new() } else { run(workdir, shadow, "diff", ["--cached", "--name-only", "--no-renames", "-z", "--"])?.stdout };
  let saved_stage: HashSet<_> = paths(&existing_stage).collect();

  // Register new paths in the isolated index so Git's path-only commit accepts them. Git reads their actual content at commit time.
  let selected: Vec<_> = files.iter().map(|file| literal(file.as_os_str())).collect();
  if !files.is_empty() {
    let tracked = run(
      workdir,
      shadow,
      "ls-files",
      [OsStr::new("--cached"), OsStr::new("-z"), OsStr::new("--")].into_iter().chain(selected.iter().map(OsString::as_os_str)),
    )?;
    let tracked: HashSet<_> = paths(&tracked.stdout).collect();
    let mut new_paths = Vec::new();
    for (file, pathspec) in files.iter().zip(&selected) {
      let known = tracked.contains(file.as_os_str());
      match fs::symlink_metadata(workdir.join(file)) {
        Ok(metadata) if metadata.is_dir() && !known => return Err(format!("Select individual files inside {}.", file.display())),
        Ok(_) if !known => new_paths.push(pathspec.as_os_str()),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not read {}: {error}", file.display())),
      }
    }
    if !new_paths.is_empty() {
      run(workdir, shadow, "add", [OsStr::new("--intent-to-add"), OsStr::new("--")].into_iter().chain(new_paths))?;
    }
  }
  let message_path = index.scratch.path().join("message");
  let description = description.trim();
  let text = if description.is_empty() { summary.to_owned() } else { format!("{summary}\n\n{description}") };
  fs::write(&message_path, text).map_err(|error| format!("Could not prepare the commit message: {error}"))?;
  super::operation::require_idle(&repository)?;
  if Head::read(&repository)? != before {
    return Err("HEAD changed while preparing the commit. Review the selection and try again.".into());
  }

  let mut arguments = vec![OsStr::new("--cleanup=verbatim"), OsStr::new("--file"), message_path.as_os_str()];
  if amend {
    arguments.push(OsStr::new("--amend"));
  }
  if !staged {
    arguments.extend([OsStr::new("--only"), OsStr::new("--")]);
    arguments.extend(selected.iter().map(OsString::as_os_str));
  }
  let output = execute(workdir, shadow, "commit", arguments)?;
  let after = Head::read(&repository);
  if !output.status.success() {
    let error = failure(&output);
    if after.as_ref().is_ok_and(|after| after == &before) {
      return Err(error);
    }
    if let Ok(Head { tip: Some(id), .. }) = after {
      return Err(format!("Git reported an error and HEAD moved to {}. Review history and staged changes before retrying. {error}", short(id)));
    }
    return Err(format!("Git reported a commit error and HEAD could not be verified. Review the repository before retrying. {error}"));
  }

  let Ok(after @ Head { tip: Some(id), .. }) = after else {
    return Ok(CommitOutcome {
      id: String::new(),
      warning: Some("Git reported success, but the new HEAD could not be read. Refresh the repository before making another commit.".into()),
    });
  };
  if after.branch != before.branch {
    return Ok(CommitOutcome {
      id: short(id),
      warning: Some("Git reported success, but the checked-out branch changed. Review the commit and index.".into()),
    });
  }
  let result = (|| {
    if !staged {
      reconcile_index(workdir, shadow, before.tip, id, selected, &saved_stage)?;
    }
    if Head::read(&repository)? != after {
      return Err("HEAD moved again before the index could be published.".into());
    }
    index.publish()
  })();
  Ok(CommitOutcome {
    id: short(id),
    warning: result
      .err()
      .map(|error| format!("Committed {}, but could not update the index. Review staged changes before the next commit. {error}", short(id))),
  })
}

fn reconcile_index(
  workdir: &Path,
  shadow: &Path,
  old: Option<Oid>,
  id: Oid,
  mut reset: Vec<OsString>,
  saved_stage: &HashSet<&OsStr>,
) -> Result<(), String> {
  let tip = id.to_string();
  let mut diff = vec!["--root", "--no-commit-id", "--name-only", "--no-renames", "-r", "-z"];
  let previous = old.map(|id| id.to_string());
  diff.extend(previous.as_deref());
  diff.push(&tip);
  let changes = run(workdir, shadow, "diff-tree", diff)?;
  // The hook may stage a fix or add a generated file. Align newly committed paths unless they had unrelated staging.
  for path in paths(&changes.stdout) {
    if !saved_stage.contains(path) {
      let path = literal(path);
      if !reset.contains(&path) {
        reset.push(path);
      }
    }
  }
  if !reset.is_empty() {
    run(
      workdir,
      shadow,
      "reset",
      [OsStr::new("--quiet"), OsStr::new(&tip), OsStr::new("--")].into_iter().chain(reset.iter().map(OsString::as_os_str)),
    )?;
  }
  Ok(())
}

pub(crate) fn literal(path: &OsStr) -> OsString {
  let mut spec = OsString::from(":(top,literal)");
  spec.push(path);
  spec
}

pub(crate) fn paths(output: &[u8]) -> impl Iterator<Item = &OsStr> {
  output.split(|byte| *byte == 0).filter(|path| !path.is_empty()).map(OsStr::from_bytes)
}

fn short(id: Oid) -> String {
  id.to_string()[..7].to_owned()
}

fn message(error: git2::Error) -> String {
  error.message().to_owned()
}

fn failure(output: &Output) -> String {
  let mut lines: Vec<String> = [output.stderr.as_slice(), output.stdout.as_slice()]
    .into_iter()
    .flat_map(|bytes| String::from_utf8_lossy(bytes).lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect::<Vec<_>>())
    .collect();
  lines.truncate(12);
  if lines.is_empty() {
    format!("Git exited with {}.", output.status)
  } else {
    lines.join("\n").chars().take(3000).collect()
  }
}

pub(crate) fn run(workdir: &Path, index: &Path, command: &str, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Result<Output, String> {
  let output = execute(workdir, index, command, args)?;
  if output.status.success() {
    Ok(output)
  } else {
    Err(failure(&output))
  }
}

fn execute(workdir: &Path, index: &Path, command: &str, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Result<Output, String> {
  ws_activity::timed(
    "git",
    || super::describe_git(workdir, &[command]),
    || {
      let mut git = Command::new("git");
      super::shell_path::apply(&mut git);
      git
        .arg("-C")
        .arg(workdir)
        .arg(command)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_LITERAL_PATHSPECS")
        .env_remove("GIT_GLOB_PATHSPECS")
        .env_remove("GIT_NOGLOB_PATHSPECS")
        .env_remove("GIT_ICASE_PATHSPECS")
        .env("GIT_INDEX_FILE", index)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not run git {command}: {error}"))
    },
    super::describe_output,
  )
}
