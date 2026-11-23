use std::{
  fs,
  path::{Path, PathBuf},
  process::Command,
};
use ws_updater::SWAP_SCRIPT;

/// The file name of the app bundle.
fn app_name() -> String {
  format!("{}.app", ws_product::NAME)
}

/// A scratch folder with a fake `open` that records what it was asked to open.
struct Scratch {
  dir: PathBuf,
}

impl Scratch {
  fn new(name: &str) -> Self {
    let dir = std::env::temp_dir().join(format!("ws-updater-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("bin/open"), format!("#!/bin/sh\necho \"$1\" > \"{}\"\n", dir.join("opened").display())).unwrap();
    Command::new("chmod").arg("+x").arg(dir.join("bin/open")).status().unwrap();
    Self { dir }
  }

  /// Runs the installer script for a process that is already gone, so it does not wait.
  fn run(&self, current: &Path, replacement: &Path, staging: &Path) {
    let script = self.dir.join("swap.sh");
    fs::write(&script, SWAP_SCRIPT).unwrap();
    let path = format!("{}:{}", self.dir.join("bin").display(), std::env::var("PATH").unwrap_or_default());
    let status = Command::new("/bin/bash").arg(&script).arg("999999").arg(current).arg(replacement).arg(staging).env("PATH", path).status().unwrap();
    assert!(status.success());
  }

  fn opened(&self) -> String {
    fs::read_to_string(self.dir.join("opened")).unwrap_or_default().trim().to_string()
  }
}

impl Drop for Scratch {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.dir);
  }
}

fn app(dir: &Path, name: &str, version: &str) -> PathBuf {
  let app = dir.join(name);
  fs::create_dir_all(&app).unwrap();
  fs::write(app.join("version"), version).unwrap();
  app
}

#[test]
fn the_installer_puts_the_new_app_in_place_removes_the_old_one_and_reopens_it() {
  let scratch = Scratch::new("swap");
  let staging = scratch.dir.join("staging");
  fs::create_dir_all(&staging).unwrap();
  let current = app(&scratch.dir, &app_name(), "old");
  let replacement = app(&staging, &app_name(), "new");
  scratch.run(&current, &replacement, &staging);
  assert_eq!(fs::read_to_string(current.join("version")).unwrap(), "new");
  assert!(!staging.exists(), "the staging folder, with the old app in it, is removed");
  assert_eq!(scratch.opened(), current.display().to_string());
}

#[test]
fn a_failed_swap_brings_the_old_app_back_and_reopens_it() {
  let scratch = Scratch::new("restore");
  let staging = scratch.dir.join("staging");
  fs::create_dir_all(&staging).unwrap();
  let current = app(&scratch.dir, &app_name(), "old");
  // The replacement is gone, so the second step of the swap fails.
  let missing = staging.join(app_name());
  scratch.run(&current, &missing, &staging);
  assert_eq!(fs::read_to_string(current.join("version")).unwrap(), "old");
  assert_eq!(scratch.opened(), current.display().to_string());
}
