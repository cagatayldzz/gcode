use std::{fs, path::PathBuf};
use vekro_git::{commit_detail, commit_file_diff, history, ChangeKind};

fn repository(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("vekro-git-history-{name}-{}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  let repository = git2::Repository::init_opts(&dir, git2::RepositoryInitOptions::new().initial_head("main")).unwrap();
  let mut config = repository.config().unwrap();
  config.set_str("user.name", "Ada").unwrap();
  config.set_str("user.email", "ada@example.com").unwrap();
  dir
}

fn commit_all(dir: &std::path::Path, message: &str, seconds: i64) -> git2::Oid {
  let repository = git2::Repository::open(dir).unwrap();
  let mut index = repository.index().unwrap();
  index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
  index.update_all(["*"], None).unwrap();
  index.write().unwrap();
  let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
  let signature = git2::Signature::new("Ada", "ada@example.com", &git2::Time::new(seconds, 0)).unwrap();
  let parent = repository.head().ok().and_then(|head| head.peel_to_commit().ok());
  repository.commit(Some("HEAD"), &signature, &signature, message, &tree, &parent.iter().collect::<Vec<_>>()).unwrap()
}

#[test]
fn history_lists_commits_newest_first_with_author_time_and_short_id() {
  let dir = repository("order");
  fs::write(dir.join("a.txt"), "one\n").unwrap();
  commit_all(&dir, "feat: first", 1_000);
  fs::write(dir.join("a.txt"), "two\n").unwrap();
  let second = commit_all(&dir, "fix: second\n\nWith a body.", 2_000);
  let found = history(&dir, 10).unwrap();
  assert_eq!(found.iter().map(|commit| commit.summary.as_str()).collect::<Vec<_>>(), ["fix: second", "feat: first"]);
  assert_eq!((found[0].author.as_str(), found[0].time, found[0].id.len(), found[0].short.len()), ("Ada", 2_000, 40, 7));
  assert_eq!(found[0].id, second.to_string());
  assert_eq!(history(&dir, 1).unwrap().len(), 1, "the limit applies");
  assert!(found.iter().all(|commit| !commit.unpushed), "a repository without a remote has nothing to push");
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_empty_repository_has_no_history() {
  let dir = repository("empty");
  assert!(history(&dir, 10).unwrap().is_empty());
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn commits_the_remote_does_not_have_are_marked_and_tags_are_listed() {
  let remote = std::env::temp_dir().join(format!("vekro-git-history-remote-{}", std::process::id()));
  let _ = fs::remove_dir_all(&remote);
  git2::Repository::init_opts(&remote, git2::RepositoryInitOptions::new().bare(true).initial_head("main")).unwrap();
  let dir = repository("unpushed");
  fs::write(dir.join("a.txt"), "one\n").unwrap();
  let first = commit_all(&dir, "first", 1_000);
  git2::Repository::open(&dir).unwrap().remote("origin", remote.to_str().unwrap()).unwrap();
  let all_unpushed = history(&dir, 10).unwrap();
  assert!(all_unpushed[0].unpushed, "without an upstream, nothing is on the remote");

  vekro_git::push(&dir, None, |_| {}).unwrap();
  fs::write(dir.join("a.txt"), "two\n").unwrap();
  commit_all(&dir, "second", 2_000);
  let repository = git2::Repository::open(&dir).unwrap();
  let target = repository.find_object(first, None).unwrap();
  repository.tag_lightweight("v0.1.0", &target, false).unwrap();
  let found = history(&dir, 10).unwrap();
  assert_eq!((found[0].unpushed, found[1].unpushed), (true, false));
  assert_eq!(found[1].tags, vec!["v0.1.0".to_string()]);
  assert!(found[0].tags.is_empty());
  fs::remove_dir_all(dir).unwrap();
  fs::remove_dir_all(remote).unwrap();
}

#[test]
fn a_commit_lists_its_files_with_kinds_and_line_totals() {
  let dir = repository("detail");
  fs::write(dir.join("keep.txt"), "a\nb\n").unwrap();
  fs::write(dir.join("gone.txt"), "x\n").unwrap();
  let first = commit_all(&dir, "first", 1_000);
  fs::write(dir.join("keep.txt"), "a\nB\nc\n").unwrap();
  fs::remove_file(dir.join("gone.txt")).unwrap();
  fs::write(dir.join("new.txt"), "n\n").unwrap();
  let second = commit_all(&dir, "second line\n\nThe body\nof it.", 2_000);

  let detail = commit_detail(&dir, &second.to_string()).unwrap();
  assert_eq!(detail.info.summary, "second line");
  assert_eq!(detail.body, "The body\nof it.");
  let files: Vec<_> = detail.files.iter().map(|file| (file.path.to_str().unwrap(), file.kind)).collect();
  assert_eq!(files, [("gone.txt", ChangeKind::Deleted), ("keep.txt", ChangeKind::Modified), ("new.txt", ChangeKind::New)]);
  assert_eq!((detail.additions, detail.deletions), (3, 2));

  let initial = commit_detail(&dir, &first.to_string()).unwrap();
  assert_eq!(initial.files.len(), 2, "a first commit is shown against nothing");
  assert!(initial.files.iter().all(|file| file.kind == ChangeKind::New));
  assert!(commit_detail(&dir, "not an id").is_err());
  fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_diff_of_one_file_in_a_commit_shows_only_that_file() {
  let dir = repository("filediff");
  fs::write(dir.join("a.txt"), "one\n").unwrap();
  fs::write(dir.join("b.txt"), "other\n").unwrap();
  commit_all(&dir, "first", 1_000);
  fs::write(dir.join("a.txt"), "two\n").unwrap();
  fs::write(dir.join("b.txt"), "changed\n").unwrap();
  let second = commit_all(&dir, "second", 2_000);
  let patch = commit_file_diff(&dir, &second.to_string(), std::path::Path::new("a.txt")).unwrap();
  assert!(patch.contains("diff --git a/a.txt b/a.txt") && patch.contains("-one") && patch.contains("+two"));
  assert!(!patch.contains("b.txt"));
  fs::remove_dir_all(dir).unwrap();
}
