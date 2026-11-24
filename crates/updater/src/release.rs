use crate::{process, UpdateError};
use std::{
  sync::Mutex,
  time::{Duration, Instant},
};
use ws_version::is_newer;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const CHECKSUMS_NAME: &str = "SHA256SUMS.txt";
const MAX_NOTES: usize = 4000;

/// How long the answer of a check is used again without asking GitHub (unauthenticated requests to its API are limited to 60 an hour
/// per address, and a release does not appear every few minutes).
const CHECK_FRESH_FOR: Duration = Duration::from_secs(600);

/// How long a check is not repeated after GitHub said it is limiting requests.
const CHECK_COOL_DOWN: Duration = Duration::from_secs(900);

/// The last check: when, for which running version, and what it found.
static LAST_CHECK: Mutex<Option<LastCheck>> = Mutex::new(None);

struct LastCheck {
  at: Instant,
  version: String,
  outcome: Result<Option<Release>, UpdateError>,
  fresh_for: Duration,
}

/// A published release that is newer than the running version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
  pub version: String,
  pub tag: String,
  pub notes: String,
  pub page_url: String,
  pub archive_name: String,
  pub archive_url: String,
  pub checksums_url: String,
}

impl Release {
  /// The release notes without Markdown markers: headings lose their `#`, bullets
  /// become `•`, and links keep only their text.
  pub fn plain_notes(&self) -> String {
    self.notes.lines().map(plain_line).collect::<Vec<_>>().join("\n").trim().to_string()
  }
}

fn plain_line(line: &str) -> String {
  let line = line.trim();
  let line = line.trim_start_matches('#').trim_start();
  let line = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).map(|rest| format!("• {rest}")).unwrap_or_else(|| line.to_string());
  strip_links(&line)
}

/// Replaces `[text](url)` with `text`, and drops a link that is the only content of parentheses.
fn strip_links(text: &str) -> String {
  let mut output = String::new();
  let mut rest = text;
  while let Some(open) = rest.find('[') {
    let (before, after) = rest.split_at(open);
    match after.find("](").and_then(|mid| after[mid..].find(')').map(|end| (mid, mid + end))) {
      Some((mid, end)) => {
        let label = &after[1..mid];
        let bare = before.ends_with('(') && after[end + 1..].starts_with(')');
        output.push_str(&before[..before.len() - usize::from(bare)]);
        output.push_str(&if bare { format!("({label})") } else { label.to_string() });
        rest = &after[end + 1 + usize::from(bare)..];
      }
      None => {
        output.push_str(before);
        output.push('[');
        rest = &after[1..];
      }
    }
  }
  output.push_str(rest);
  output
}

/// `owner/name` of the GitHub repository that publishes releases.
pub fn repository_slug() -> Option<&'static str> {
  REPOSITORY.strip_prefix("https://github.com/").map(|slug| slug.trim_end_matches('/').trim_end_matches(".git")).filter(|s| s.contains('/'))
}

/// The newest published release when it is newer than `current`; `None` when
/// The app is up to date or no release exists yet. The answer is kept for ten minutes, so checking again (at launch, then by hand)
/// does not ask GitHub again; a refusal because of its rate limit is kept for fifteen.
pub fn check(current: &str) -> Result<Option<Release>, UpdateError> {
  let kept = LAST_CHECK.lock().unwrap_or_else(|error| error.into_inner());
  if let Some(LastCheck { at, version, outcome, fresh_for }) = kept.as_ref() {
    if version == current && at.elapsed() < *fresh_for {
      ws_activity::record("http", || "check for a newer release → kept from the last check, GitHub not asked again".into());
      return outcome.clone();
    }
  }
  // Held while asking, so a check that starts meanwhile waits for this one instead of sending its own.
  let outcome = ws_activity::timed(
    "http",
    || "GET releases/latest (check for a newer release)".into(),
    || ask(current),
    |result| match result {
      Ok(Some(release)) => format!("ok, {} is available", release.tag),
      Ok(None) => "ok, up to date".into(),
      Err(error) => format!("failed: {error}"),
    },
  );
  let limited = outcome.as_ref().is_err_and(|error| error.to_string().contains("rate limiting"));
  let mut kept = kept;
  let fresh_for = if limited { CHECK_COOL_DOWN } else { CHECK_FRESH_FOR };
  *kept = (outcome.is_ok() || limited).then(|| LastCheck { at: Instant::now(), version: current.to_string(), outcome: outcome.clone(), fresh_for });
  outcome
}

fn ask(current: &str) -> Result<Option<Release>, UpdateError> {
  if !cfg!(target_arch = "aarch64") {
    return Err(UpdateError::new(format!("{} releases are published for Apple Silicon Macs only.", ws_product::NAME)));
  }
  let slug = repository_slug().ok_or_else(|| UpdateError::new("The release repository is not configured."))?;
  let (status, body) = process::get(&format!("https://api.github.com/repos/{slug}/releases/latest"))?;
  match status {
    200 => {}
    404 => return Ok(None),
    403 | 429 => return Err(UpdateError::new("GitHub is rate limiting requests. Try again later.")),
    other => return Err(UpdateError::new(format!("GitHub answered with status {other}."))),
  }
  let release = parse_release(&body, slug)?;
  Ok(is_newer(&release.version, current).then_some(release))
}

/// Reads a `releases/latest` response. Asset URLs must be GitHub release
/// downloads of this repository and tag, so a tampered response cannot point the
/// updater elsewhere.
pub fn parse_release(json: &str, slug: &str) -> Result<Release, UpdateError> {
  let invalid = || UpdateError::new("The latest release is missing required information.");
  let value: serde_json::Value = serde_json::from_str(json).map_err(|_| UpdateError::new("Could not read the release information."))?;
  let tag = value["tag_name"].as_str().ok_or_else(invalid)?;
  let version = tag.strip_prefix('v').filter(|v| is_plain_version(v)).ok_or_else(invalid)?;
  let archive_name = format!("{}-{tag}-macos-arm64.zip", ws_product::NAME);
  let prefix = format!("https://github.com/{slug}/releases/download/{tag}/");
  let asset = |name: &str| -> Result<String, UpdateError> {
    let url =
      value["assets"].as_array().and_then(|assets| assets.iter().find(|a| a["name"] == name)).and_then(|a| a["browser_download_url"].as_str());
    url.filter(|url| *url == format!("{prefix}{name}")).map(str::to_string).ok_or_else(invalid)
  };
  Ok(Release {
    version: version.to_string(),
    tag: tag.to_string(),
    notes: value["body"].as_str().unwrap_or_default().chars().take(MAX_NOTES).collect(),
    page_url: value["html_url"].as_str().filter(|url| url.starts_with(&format!("https://github.com/{slug}/"))).unwrap_or_default().to_string(),
    archive_url: asset(&archive_name)?,
    checksums_url: asset(CHECKSUMS_NAME)?,
    archive_name,
  })
}

fn is_plain_version(version: &str) -> bool {
  let parts: Vec<_> = version.split('.').collect();
  parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.len() < 10 && p.chars().all(|c| c.is_ascii_digit()))
}

/// The hash recorded for `file` in a `shasum` listing (`<hash>  <file>`).
pub fn checksum_for(listing: &str, file: &str) -> Option<String> {
  listing.lines().find_map(|line| {
    let (hash, name) = line.split_once(char::is_whitespace)?;
    let name = name.trim().trim_start_matches('*');
    (name == file && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())).then(|| hash.to_ascii_lowercase())
  })
}
