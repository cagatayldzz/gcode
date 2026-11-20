use std::{fs, path::PathBuf};
use vekro_storage::{Profile, ProfileList, DEFAULT_PROFILE};

fn scratch(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("vekro-profiles-{name}-{}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  dir
}

#[test]
fn a_missing_file_gives_the_default_profile_alone() {
  let list = ProfileList::load(&scratch("missing").join("profiles.json"));
  assert_eq!(list, ProfileList::default());
  assert_eq!(list.last_used(), Profile::default_profile());
}

#[test]
fn profiles_round_trip_and_keep_the_last_used() {
  let dir = scratch("round-trip");
  let mut list = ProfileList::default();
  let work = list.add("Work").unwrap();
  list.set_last(&work.id);
  list.save(&dir.join("profiles.json")).unwrap();
  let loaded = ProfileList::load(&dir.join("profiles.json"));
  assert_eq!(loaded, list);
  assert_eq!(loaded.last_used(), work);
}

#[test]
fn ids_are_unique_and_safe_as_folder_names() {
  let mut list = ProfileList::default();
  let first = list.add("My Work / Safebit").unwrap();
  let second = list.add("My Work / Safebit").unwrap();
  assert_eq!(first.id, "my-work-safebit");
  assert_eq!(second.id, "my-work-safebit-2");
  assert!(list.add("   ").is_none());
  assert_eq!(list.add("***").unwrap().id, "profile");
}

#[test]
fn the_default_profile_is_always_listed_and_cannot_be_removed() {
  let dir = scratch("default");
  fs::write(dir.join("profiles.json"), r#"{"profiles":[{"id":"work","name":"Work"},{"id":"../evil","name":"Evil"}],"last":"gone"}"#).unwrap();
  let mut list = ProfileList::load(&dir.join("profiles.json"));
  assert_eq!(list.profiles.iter().map(|profile| profile.id.as_str()).collect::<Vec<_>>(), [DEFAULT_PROFILE, "work"]);
  assert_eq!(list.last, DEFAULT_PROFILE);
  assert!(list.remove(DEFAULT_PROFILE).is_none());
  assert_eq!(list.profiles.len(), 2);
}

#[test]
fn removing_the_last_used_profile_falls_back_to_the_default() {
  let mut list = ProfileList::default();
  let work = list.add("Work").unwrap();
  list.set_last(&work.id);
  assert_eq!(list.remove(&work.id), Some(work));
  assert_eq!(list.last, DEFAULT_PROFILE);
}

#[test]
fn renaming_changes_the_name_but_not_the_id() {
  let mut list = ProfileList::default();
  let work = list.add("Work").unwrap();
  list.rename(&work.id, "  Safebit ");
  list.rename(&work.id, "   ");
  assert_eq!(list.get(&work.id).unwrap().name, "Safebit");
}

#[test]
fn only_the_default_profile_uses_the_data_directory_itself() {
  let default = Profile::default_profile();
  let other = Profile { id: "work".into(), name: "Work".into() };
  assert_eq!(other.dir(), default.dir().join("profiles").join("work"));
  assert_eq!(default.keychain_namespace(), None);
  assert_eq!(other.keychain_namespace().as_deref(), Some("work"));
}
