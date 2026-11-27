//! Discover project commands without evaluating manifests or starting programs.

use serde_json::Value;
use std::{
  fs::File,
  io::{self, Read},
  path::Path,
};

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
  pub source: &'static str,
  pub name: String,
  pub program: &'static str,
  pub arguments: Vec<String>,
  pub body: Option<String>,
}

impl Script {
  pub fn command(&self) -> String {
    format!("{} {}", self.program, self.arguments.join(" "))
  }

  /// Use the login shell's PATH while keeping every script name a single literal argument.
  pub fn invocation(&self, shell: String) -> (String, Vec<String>) {
    let mut arguments = vec!["-lc".into(), "exec \"$@\"".into(), "project-script".into(), self.program.into()];
    arguments.extend(self.arguments.clone());
    (shell, arguments)
  }
}

pub fn discover(root: &Path) -> Result<Vec<Script>, String> {
  let mut scripts = Vec::new();
  if let Some(text) = read_manifest(&root.join("package.json")).map_err(|error| format!("Could not read package.json: {error}"))? {
    let package: Value = serde_json::from_str(&text).map_err(|error| format!("Could not read package.json: {error}"))?;
    let package = package.as_object().ok_or("package.json must contain a JSON object.")?;
    let manager = ["bun", "pnpm", "yarn", "npm"]
      .into_iter()
      .find(|manager| {
        package.get("packageManager").and_then(Value::as_str).and_then(|value| value.split_once('@')).is_some_and(|(name, _)| name == *manager)
      })
      .or_else(|| {
        [("bun.lock", "bun"), ("bun.lockb", "bun"), ("pnpm-lock.yaml", "pnpm"), ("yarn.lock", "yarn"), ("package-lock.json", "npm")]
          .into_iter()
          .find(|(lock, _)| root.join(lock).is_file())
          .map(|(_, manager)| manager)
      })
      .unwrap_or("npm");
    if let Some(entries) = package.get("scripts").and_then(Value::as_object) {
      scripts.extend(
        entries
          .iter()
          .filter(|(name, body)| body.is_string() && !name.is_empty() && !name.starts_with('-') && !name.chars().any(char::is_control))
          .map(|(name, body)| Script {
            source: "package.json",
            name: name.clone(),
            program: manager,
            arguments: vec!["run".into(), name.clone()],
            body: body.as_str().map(str::to_owned),
          }),
      );
    }
  }
  if root.join("Cargo.toml").is_file() {
    scripts.extend(["build", "check", "test"].into_iter().map(|name| Script {
      source: "Cargo.toml",
      name: name.into(),
      program: "cargo",
      arguments: vec![name.into()],
      body: None,
    }));
  }
  for source in ["GNUmakefile", "Makefile", "makefile"] {
    let Some(text) = read_manifest(&root.join(source)).map_err(|error| format!("Could not read {source}: {error}"))? else {
      continue;
    };
    let mut targets = std::collections::BTreeSet::new();
    for line in text.lines() {
      if line.starts_with(char::is_whitespace) {
        continue;
      }
      let Some((target, rest)) = line.split_once(':') else { continue };
      if rest.trim_start().starts_with('=') || rest.starts_with(":=") {
        continue;
      }
      for target in target.split_whitespace().filter(|target| {
        target.starts_with(|character: char| character.is_ascii_alphanumeric())
          && target.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/'))
      }) {
        targets.insert(target);
      }
    }
    scripts.extend(targets.into_iter().map(|target| Script {
      source,
      name: target.into(),
      program: "make",
      arguments: vec![target.into()],
      body: None,
    }));
    break;
  }
  scripts.sort_by(|left, right| (left.source, &left.name).cmp(&(right.source, &right.name)));
  Ok(scripts)
}

fn read_manifest(path: &Path) -> io::Result<Option<String>> {
  let metadata = match path.metadata() {
    Ok(metadata) => metadata,
    Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
    Err(error) => return Err(error),
  };
  if !metadata.is_file() {
    return Err(io::Error::other("The manifest is not a regular file."));
  }
  let mut text = String::new();
  File::open(path)?.take(MAX_MANIFEST_BYTES + 1).read_to_string(&mut text)?;
  if text.len() as u64 > MAX_MANIFEST_BYTES {
    return Err(io::Error::other("The manifest is too large."));
  }
  Ok(Some(text))
}
