//! Turns `product.toml` at the repository root into constants.

use std::{env, fs, path::PathBuf};

/// The value of `key = "value"` in the file; an error names what is missing, so a typo fails the build.
fn value(text: &str, key: &str) -> String {
  text
    .lines()
    .filter_map(|line| line.split_once('='))
    .find(|(name, _)| name.trim() == key)
    .map(|(_, value)| value.split('#').next().unwrap_or_default().trim().trim_matches('"').to_string())
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| panic!("product.toml has no `{key}`"))
}

fn main() {
  let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets the manifest directory"));
  let file = manifest.join("../../product.toml");
  println!("cargo:rerun-if-changed={}", file.display());
  let text = fs::read_to_string(&file).unwrap_or_else(|error| panic!("could not read {}: {error}", file.display()));
  let constants: String = ["name", "env_prefix", "bundle_id", "legacy_name"]
    .iter()
    .map(|key| format!("pub const {}: &str = {:?};\n", key.to_uppercase(), value(&text, key)))
    .collect();
  let out = PathBuf::from(env::var("OUT_DIR").expect("cargo sets the output directory")).join("product.rs");
  fs::write(out, constants).expect("could not write the constants");
}
