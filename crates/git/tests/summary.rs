use std::{fs, path::PathBuf};
use ws_git::summary;

fn temp_dir(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("ws-git-{name}-{}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  dir
}

#[test]
fn a_folder_outside_a_repository_has_no_summary() {
  let dir = temp_dir("plain");
  assert_eq!(summary(&dir), None);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_new_repository_names_its_branch_and_counts_untracked_files() {
  let dir = temp_dir("fresh");
  git2::Repository::init_opts(&dir, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  fs::write(dir.join("a.txt"), "a").unwrap();
  fs::write(dir.join("b.txt"), "b").unwrap();
  let found = summary(&dir).unwrap();
  assert_eq!((found.branch.as_str(), found.changed()), ("main", 2));
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn repository_names_come_from_the_last_url_segment() {
  use ws_git::repository_name;
  assert_eq!(repository_name("https://github.com/owner/project.git"), Some("project".into()));
  assert_eq!(repository_name("git@github.com:owner/project.git"), Some("project".into()));
  assert_eq!(repository_name(" https://github.com/owner/project/ "), Some("project".into()));
  assert_eq!(repository_name(""), None);
}

#[test]
fn a_created_project_is_a_repository_on_main_and_cannot_be_created_twice() {
  let parent = temp_dir("create");
  let path = ws_git::create_project(&parent, "demo").unwrap();
  assert_eq!(summary(&path).unwrap().branch, "main");
  assert!(ws_git::create_project(&parent, "demo").is_err());
  assert!(ws_git::create_project(&parent, "a/b").is_err());
  fs::remove_dir_all(parent).unwrap();
}

#[test]
fn cloning_copies_a_repository_and_reports_a_missing_one() {
  let parent = temp_dir("clone");
  let source = ws_git::create_project(&parent, "source").unwrap();
  let destination = parent.join("elsewhere");
  fs::create_dir_all(&destination).unwrap();
  let cloned = ws_git::clone(source.to_str().unwrap(), &destination, None).unwrap();
  assert_eq!(cloned, destination.join("source"));
  assert!(ws_git::clone(parent.join("missing").to_str().unwrap(), &destination, None).is_err());
  fs::remove_dir_all(parent).unwrap();
}

#[test]
fn changes_list_new_modified_and_deleted_files_with_a_diff_for_each() {
  use ws_git::{file_diff, ChangeKind};
  let dir = temp_dir("changes");
  let repository = git2::Repository::init_opts(&dir, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  fs::write(dir.join("kept.txt"), "one\n").unwrap();
  fs::write(dir.join("gone.txt"), "bye\n").unwrap();
  let mut index = repository.index().unwrap();
  index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
  index.write().unwrap();
  let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
  let signature = git2::Signature::now("Test", "test@example.com").unwrap();
  repository.commit(Some("HEAD"), &signature, &signature, "first", &tree, &[]).unwrap();

  fs::write(dir.join("kept.txt"), "one\ntwo\n").unwrap();
  fs::remove_file(dir.join("gone.txt")).unwrap();
  fs::write(dir.join("fresh.txt"), "new\n").unwrap();

  let found = summary(&dir).unwrap();
  let kinds: Vec<_> = found.changes.iter().map(|change| (change.path.to_str().unwrap().to_string(), change.kind)).collect();
  assert_eq!(
    kinds,
    [("fresh.txt".to_string(), ChangeKind::New), ("gone.txt".to_string(), ChangeKind::Deleted), ("kept.txt".to_string(), ChangeKind::Modified)]
  );
  let patch = file_diff(&dir, std::path::Path::new("kept.txt")).unwrap();
  assert!(patch.contains("+two"), "{patch}");
  assert!(file_diff(&dir, std::path::Path::new("fresh.txt")).unwrap().contains("+new"));
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn repositories_directly_inside_a_folder_are_found_by_name() {
  let parent = temp_dir("find");
  ws_git::create_project(&parent, "beta").unwrap();
  ws_git::create_project(&parent, "Alpha").unwrap();
  fs::create_dir_all(parent.join("plain-folder")).unwrap();
  fs::write(parent.join("a-file.txt"), "x").unwrap();
  let found: Vec<_> = ws_git::find_repositories(&parent).iter().map(|path| path.file_name().unwrap().to_str().unwrap().to_string()).collect();
  assert_eq!(found, ["Alpha", "beta"]);
  assert!(ws_git::find_repositories(&parent.join("missing")).is_empty());
  fs::remove_dir_all(parent).unwrap();
}

fn committed_repository(name: &str) -> PathBuf {
  let dir = temp_dir(name);
  let repository = git2::Repository::init_opts(&dir, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  fs::write(dir.join("kept.txt"), "original").unwrap();
  fs::write(dir.join("other.txt"), "other").unwrap();
  let mut index = repository.index().unwrap();
  index.add_path(std::path::Path::new("kept.txt")).unwrap();
  index.add_path(std::path::Path::new("other.txt")).unwrap();
  index.write().unwrap();
  let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
  let signature = git2::Signature::now("Test", "test@example.com").unwrap();
  repository.commit(Some("HEAD"), &signature, &signature, "first", &tree, &[]).unwrap();
  dir
}

#[test]
fn discarding_restores_a_modified_or_deleted_file_and_leaves_the_others_alone() {
  use ws_git::discard_changes;
  let dir = committed_repository("discard-tracked");
  fs::write(dir.join("kept.txt"), "edited").unwrap();
  fs::write(dir.join("other.txt"), "edited too").unwrap();
  discard_changes(&dir, std::path::Path::new("kept.txt")).unwrap();
  assert_eq!(fs::read_to_string(dir.join("kept.txt")).unwrap(), "original");
  assert_eq!(fs::read_to_string(dir.join("other.txt")).unwrap(), "edited too");

  fs::remove_file(dir.join("other.txt")).unwrap();
  discard_changes(&dir, std::path::Path::new("other.txt")).unwrap();
  assert_eq!(fs::read_to_string(dir.join("other.txt")).unwrap(), "other");
  assert_eq!(summary(&dir).unwrap().changed(), 0);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn discarding_a_new_file_deletes_it_even_when_it_was_staged() {
  use ws_git::discard_changes;
  let dir = committed_repository("discard-new");
  fs::write(dir.join("fresh.txt"), "new").unwrap();
  fs::write(dir.join("staged.txt"), "new").unwrap();
  let repository = git2::Repository::open(&dir).unwrap();
  let mut index = repository.index().unwrap();
  index.add_path(std::path::Path::new("staged.txt")).unwrap();
  index.write().unwrap();

  discard_changes(&dir, std::path::Path::new("fresh.txt")).unwrap();
  discard_changes(&dir, std::path::Path::new("staged.txt")).unwrap();
  assert!(!dir.join("fresh.txt").exists());
  assert!(!dir.join("staged.txt").exists());
  assert_eq!(summary(&dir).unwrap().changed(), 0);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_changed_file_is_found_by_its_repository_relative_path() {
  let dir = committed_repository("workdir-file");
  let found = ws_git::workdir_file(&dir, std::path::Path::new("kept.txt")).unwrap();
  assert!(found.ends_with("kept.txt") && found.exists());
  fs::remove_dir_all(dir).unwrap();
}

fn configure_author(dir: &PathBuf) {
  let repository = git2::Repository::open(dir).unwrap();
  let mut config = repository.config().unwrap();
  config.set_str("user.name", "Test").unwrap();
  config.set_str("user.email", "test@example.com").unwrap();
}

#[test]
fn committing_takes_only_the_chosen_files() {
  use ws_git::commit;
  let dir = committed_repository("commit-chosen");
  configure_author(&dir);
  fs::write(dir.join("kept.txt"), "edited").unwrap();
  fs::write(dir.join("other.txt"), "edited too").unwrap();
  fs::write(dir.join("fresh.txt"), "new").unwrap();

  let id = commit(&dir, &[PathBuf::from("kept.txt"), PathBuf::from("fresh.txt")], "  Change two files ", "More about it").unwrap();
  assert_eq!(id.len(), 7);
  let found = summary(&dir).unwrap();
  let left: Vec<_> = found.changes.iter().map(|change| change.path.to_string_lossy().into_owned()).collect();
  assert_eq!(left, ["other.txt"]);

  let repository = git2::Repository::open(&dir).unwrap();
  let head = repository.head().unwrap().peel_to_commit().unwrap();
  assert_eq!(head.message().unwrap(), "Change two files\n\nMore about it");
  assert_eq!(head.parent_count(), 1);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_file_staged_but_not_chosen_stays_out_of_the_commit() {
  use ws_git::commit;
  let dir = committed_repository("commit-staged");
  configure_author(&dir);
  fs::write(dir.join("kept.txt"), "edited").unwrap();
  fs::write(dir.join("other.txt"), "edited too").unwrap();
  let repository = git2::Repository::open(&dir).unwrap();
  let mut index = repository.index().unwrap();
  index.add_path(std::path::Path::new("other.txt")).unwrap();
  index.write().unwrap();

  commit(&dir, &[PathBuf::from("kept.txt")], "Only one", "").unwrap();
  let head_tree = repository.head().unwrap().peel_to_tree().unwrap();
  let in_commit = head_tree.get_path(std::path::Path::new("other.txt")).unwrap().to_object(&repository).unwrap();
  assert_eq!(in_commit.as_blob().unwrap().content(), b"other");
  assert_eq!(summary(&dir).unwrap().changed(), 1);
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn committing_handles_deleted_files_the_first_commit_and_bad_input() {
  use ws_git::commit;
  let dir = committed_repository("commit-edges");
  configure_author(&dir);
  fs::remove_file(dir.join("other.txt")).unwrap();
  commit(&dir, &[PathBuf::from("other.txt")], "Remove it", "").unwrap();
  assert_eq!(summary(&dir).unwrap().changed(), 0);
  assert!(commit(&dir, &[], "No files", "").is_err());
  assert!(commit(&dir, &[PathBuf::from("kept.txt")], "   ", "").is_err());
  assert!(commit(&dir, &[PathBuf::from("kept.txt")], "Nothing changed", "").is_err());

  let fresh = temp_dir("commit-first");
  git2::Repository::init_opts(&fresh, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  configure_author(&fresh);
  fs::write(fresh.join("a.txt"), "a").unwrap();
  commit(&fresh, &[PathBuf::from("a.txt")], "First", "").unwrap();
  assert_eq!(summary(&fresh).unwrap().changed(), 0);
  fs::remove_dir_all(dir).unwrap();
  fs::remove_dir_all(fresh).unwrap();
}

#[test]
fn changing_the_remote_address_keeps_the_remote_and_adds_origin_when_missing() {
  use ws_git::{remote_url, set_remote_url};
  let dir = temp_dir("set-remote");
  assert!(set_remote_url(&dir, "https://github.com/a/b.git").is_err());
  git2::Repository::init_opts(&dir, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  assert_eq!(remote_url(&dir), None);
  set_remote_url(&dir, "https://github.com/cagatayldzz/relvo.git").unwrap();
  assert_eq!(remote_url(&dir).as_deref(), Some("https://github.com/cagatayldzz/relvo.git"));
  set_remote_url(&dir, " https://github.com/cagatayldzz/appforge.git ").unwrap();
  assert_eq!(remote_url(&dir).as_deref(), Some("https://github.com/cagatayldzz/appforge.git"));
  assert_eq!(git2::Repository::open(&dir).unwrap().remotes().unwrap().len(), 1);
  assert!(set_remote_url(&dir, "  ").is_err());
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_staged_rename_is_one_change_at_the_new_path_and_its_diff_shows_only_the_edit() {
  use std::path::Path;
  use ws_git::{renamed_file_diff, ChangeKind};

  let dir = temp_dir("rename");
  let repository = git2::Repository::init_opts(&dir, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  let body: String = (0..30).map(|line| format!("line {line}\n")).collect();
  fs::write(dir.join("old.txt"), &body).unwrap();
  let mut index = repository.index().unwrap();
  index.add_path(Path::new("old.txt")).unwrap();
  let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
  let signature = git2::Signature::now("Test", "test@example.com").unwrap();
  repository.commit(Some("HEAD"), &signature, &signature, "first", &tree, &[]).unwrap();

  fs::rename(dir.join("old.txt"), dir.join("new.txt")).unwrap();
  fs::write(dir.join("new.txt"), body.replace("line 3\n", "line three\n")).unwrap();
  index.remove_path(Path::new("old.txt")).unwrap();
  index.add_path(Path::new("new.txt")).unwrap();
  index.write().unwrap();

  let found = summary(&dir).unwrap();
  assert_eq!(found.changes.len(), 1, "{:?}", found.changes);
  let change = &found.changes[0];
  assert_eq!((change.path.as_path(), change.kind, change.from.as_deref()), (Path::new("new.txt"), ChangeKind::Renamed, Some(Path::new("old.txt"))));
  let patch = renamed_file_diff(&dir, Path::new("new.txt"), Path::new("old.txt")).unwrap();
  assert!(patch.contains("+line three") && patch.contains("-line 3"), "{patch}");
  assert!(!patch.contains("-line 4"), "{patch}");
  fs::remove_dir_all(dir).unwrap();
}
