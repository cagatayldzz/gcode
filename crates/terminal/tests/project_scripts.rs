use std::fs;
use ws_terminal::project_scripts::discover;

#[test]
fn discovers_package_scripts_with_the_configured_runner_before_consulting_lockfiles() {
  let dir = tempfile::tempdir().unwrap();
  fs::write(dir.path().join("bun.lock"), "").unwrap();
  fs::write(
    dir.path().join("package.json"),
    r#"{"packageManager":"pnpm@9","scripts":{"dev:api":"node server.js","--version":"bad","name;echo":"one argument","invalid":false}}"#,
  )
  .unwrap();
  let scripts = discover(dir.path()).unwrap();
  assert_eq!(scripts.iter().map(|script| script.name.as_str()).collect::<Vec<_>>(), ["dev:api", "name;echo"]);
  assert_eq!(scripts[0].program, "pnpm");
  assert_eq!(scripts[0].body.as_deref(), Some("node server.js"));
  assert_eq!(scripts[1].invocation("/bin/sh".into()).1, ["-lc", "exec \"$@\"", "project-script", "pnpm", "run", "name;echo"]);
}

#[test]
fn uses_lockfiles_and_discovers_cargo_and_literal_make_targets() {
  let dir = tempfile::tempdir().unwrap();
  fs::write(dir.path().join("package.json"), r#"{"scripts":{"dev":"server"}}"#).unwrap();
  fs::write(dir.path().join("yarn.lock"), "").unwrap();
  fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
  fs::write(dir.path().join("Makefile"), ".PHONY: clean\nclean: \n\tprintf 'ignore: this'\nNAME := value\n%.o: %.c\n").unwrap();
  let scripts = discover(dir.path()).unwrap();

  assert_eq!(
    scripts.iter().map(|script| script.command()).collect::<Vec<_>>(),
    ["cargo build", "cargo check", "cargo test", "make clean", "yarn run dev"]
  );
}

#[test]
fn damaged_package_manifest_produces_a_useful_error() {
  let dir = tempfile::tempdir().unwrap();
  fs::write(dir.path().join("package.json"), "invalid json").unwrap();
  assert!(discover(dir.path()).unwrap_err().contains("package.json"));
}
