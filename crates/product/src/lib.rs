//! The product's name, in one place. Everything that shows or stores the name reads it here; the values come from `product.toml`.

include!(concat!(env!("OUT_DIR"), "/product.rs"));

/// The name of an environment variable the app reads: `<prefix>_<suffix>`.
pub fn env_name(suffix: &str) -> String {
  format!("{ENV_PREFIX}_{suffix}")
}

/// The value of one of the app's environment variables (`env_var("DATA_DIR")` reads `<prefix>_DATA_DIR`).
pub fn env_var(suffix: &str) -> Option<std::ffi::OsString> {
  std::env::var_os(env_name(suffix))
}

/// The name in lower case, for file names and commands.
pub fn slug() -> String {
  NAME.to_lowercase()
}
