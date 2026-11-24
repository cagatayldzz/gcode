//! The `PATH` of the user's login shell. An app opened from the Dock or Finder gets a short `PATH`, so hooks that run `bun`, `node` or
//! other tools the shell finds (a Husky `pre-push`, for example) fail with "command not found" when git is started from the app.

use std::{process::Command, sync::OnceLock};

const START: &str = "__PATH_START__";
const END: &str = "__PATH_END__";

/// The `PATH` the user's login shell would give a terminal, read once. `None` when the shell cannot tell.
pub fn login_shell_path() -> Option<&'static str> {
  static PATH: OnceLock<Option<String>> = OnceLock::new();
  PATH.get_or_init(read_login_shell_path).as_deref()
}

fn read_login_shell_path() -> Option<String> {
  let shell = std::env::var("SHELL").ok().filter(|shell| !shell.is_empty()).unwrap_or_else(|| "/bin/zsh".to_string());
  let output = Command::new(shell)
    .args(["-ilc", &format!("printf '%s' \"{START}$PATH{END}\"")])
    .stdin(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .output()
    .ok()?;
  extract_path(&String::from_utf8_lossy(&output.stdout))
}

/// The `PATH` between the markers; a shell may print a banner or other noise around it.
pub fn extract_path(output: &str) -> Option<String> {
  let start = output.find(START)? + START.len();
  let end = output[start..].find(END)? + start;
  let path = output[start..end].trim();
  (!path.is_empty()).then(|| path.to_string())
}

/// Gives the command the login shell's `PATH`, merged with the one it already has.
pub fn apply(command: &mut Command) {
  let Some(login) = login_shell_path() else { return };
  let merged = match std::env::var("PATH") {
    Ok(current) if !current.is_empty() => format!("{login}:{current}"),
    _ => login.to_string(),
  };
  command.env("PATH", merged);
}
