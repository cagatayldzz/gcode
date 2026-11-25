use crate::ExtensionError;
use std::process::Command;

/// The user agent of the requests: the product name and version.
fn user_agent() -> String {
  format!("{}/{}", ws_product::NAME, env!("CARGO_PKG_VERSION"))
}

/// HTTPS GET through curl; returns the status code and body. Redirects stay on HTTPS.
pub(crate) fn get(url: &str) -> Result<(u16, String), ExtensionError> {
  let output = Command::new("/usr/bin/curl")
    .args([
      "--proto",
      "=https",
      "--proto-redir",
      "=https",
      "--tlsv1.2",
      "-sSL",
      "--connect-timeout",
      "15",
      "--max-time",
      "45",
      "--max-filesize",
      "1000000",
      "--user-agent",
      &user_agent(),
      "--write-out",
      "\n%{http_code}",
      url,
    ])
    .output()
    .map_err(|error| ExtensionError::new(format!("Could not run curl: {error}")))?;
  if !output.status.success() {
    return Err(ExtensionError::new("Could not reach the extension registry. Check the connection."));
  }
  let text = String::from_utf8_lossy(&output.stdout).into_owned();
  let (body, status) = text.trim_end().rsplit_once('\n').ok_or_else(|| ExtensionError::new("Unexpected answer from the extension registry."))?;
  let status = status.trim().parse().map_err(|_| ExtensionError::new("Unexpected answer from the extension registry."))?;
  Ok((status, body.to_string()))
}
