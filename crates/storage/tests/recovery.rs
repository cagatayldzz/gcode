use std::{fs, os::unix::fs::PermissionsExt as _};
use ws_storage::{write_if_current, Draft, FileVersion, RecoveryJournal};

#[test]
fn a_private_profile_journal_restores_the_latest_draft_and_its_original_disk_version() {
  let dir = tempfile::tempdir().unwrap();
  let profile = dir.path().join("profile");
  let path = dir.path().join("document.txt");
  fs::write(&path, "base").unwrap();
  let base = FileVersion::read(&path).unwrap();
  let journal = RecoveryJournal::new(&profile);
  journal.store(&Draft::new(&path, "first edit".into(), base.clone())).unwrap();
  journal.store(&Draft::new(&path, "latest edit".into(), base.clone())).unwrap();
  fs::write(&path, "external edit").unwrap();

  let (drafts, unreadable) = RecoveryJournal::new(&profile).load().unwrap();
  assert_eq!(unreadable, 0);
  assert_eq!(drafts.len(), 1);
  assert_eq!(drafts[0].path(), path);
  assert_eq!(drafts[0].text, "latest edit");
  assert_eq!(drafts[0].base, base);
  assert!(RecoveryJournal::new(dir.path()).load().unwrap().0.is_empty());
  let recovery = profile.join("editor-recovery");
  let snapshot = fs::read_dir(&recovery).unwrap().next().unwrap().unwrap().path();
  assert_eq!(recovery.metadata().unwrap().permissions().mode() & 0o777, 0o700);
  assert_eq!(snapshot.metadata().unwrap().permissions().mode() & 0o777, 0o600);

  journal.remove(&path).unwrap();
  assert!(journal.load().unwrap().0.is_empty());
}

#[test]
fn an_overwrite_requires_the_reviewed_contents_even_when_the_file_size_is_the_same() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("file.txt");
  fs::write(&path, b"old").unwrap();
  let original = FileVersion::read(&path).unwrap();
  fs::write(&path, b"one").unwrap();
  assert!(!write_if_current(&path, b"local", original.as_ref()).unwrap());

  let reviewed = FileVersion::read(&path).unwrap();
  fs::write(&path, b"two").unwrap();
  assert!(!write_if_current(&path, b"local", reviewed.as_ref()).unwrap());
  assert_eq!(fs::read(&path).unwrap(), b"two");

  let newest = FileVersion::read(&path).unwrap();
  assert!(write_if_current(&path, b"local", newest.as_ref()).unwrap());
  assert_eq!(fs::read(&path).unwrap(), b"local");
}

#[test]
fn a_deleted_file_can_only_be_recreated_if_the_path_is_still_vacant() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("file.txt");
  fs::write(&path, b"base").unwrap();
  let base = FileVersion::read(&path).unwrap();
  fs::remove_file(&path).unwrap();
  assert!(!write_if_current(&path, b"edited", base.as_ref()).unwrap());

  let absent = FileVersion::read(&path).unwrap();
  fs::write(&path, b"replaced externally").unwrap();
  assert!(!write_if_current(&path, b"edited", absent.as_ref()).unwrap());
  assert_eq!(fs::read(&path).unwrap(), b"replaced externally");

  fs::remove_file(&path).unwrap();
  assert!(write_if_current(&path, b"edited", absent.as_ref()).unwrap());
  assert_eq!(fs::read(&path).unwrap(), b"edited");
}
