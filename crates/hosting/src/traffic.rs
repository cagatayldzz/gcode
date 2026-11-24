//! What keeps the app from asking a service the same thing twice, or too often:
//!
//! - one shared HTTP agent, so connections are reused instead of opened for every request;
//! - answers to reads are kept for [`FRESH_FOR`], and after that asked again with their `ETag` (GitHub does not count an answer of
//!   "not modified" against the rate limit);
//! - identical reads that are in flight at once share one request: the second waits for the first and takes its answer;
//! - a service that said "slow down" (HTTP 429, or its rate limit used up) is left alone until it says it is ready, and the
//!   error says when that is.

use serde_json::Value;
use std::{
  collections::HashMap,
  sync::{Arc, Mutex, MutexGuard, OnceLock},
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// How long an answer to a read is used without asking again.
pub(crate) const FRESH_FOR: Duration = Duration::from_secs(60);

/// The most answers kept; the ones nobody is using are dropped past this.
const MAX_SLOTS: usize = 256;

/// The longest a service is left alone, however far away its reset is.
const MAX_WAIT: Duration = Duration::from_secs(3600);

/// How long a `429` without a hint leaves the service alone.
const DEFAULT_WAIT: Duration = Duration::from_secs(60);

/// The agent every request goes through.
pub(crate) fn agent() -> ureq::Agent {
  static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
  AGENT.get_or_init(|| ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).http_status_as_error(false).build().into()).clone()
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
  mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// An answer to a read: the JSON and every response header (names in lower case).
#[derive(Clone)]
pub(crate) struct Answer {
  pub value: Value,
  pub headers: Vec<(String, String)>,
}

impl Answer {
  pub fn header(&self, name: &str) -> Option<String> {
    self.headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone())
  }
}

/// What is known about one URL for one account.
#[derive(Default)]
pub(crate) struct Slot {
  pub answer: Option<Answer>,
  pub etag: Option<String>,
  pub fetched: Option<Instant>,
}

impl Slot {
  /// The answer, while it is recent enough to use without asking.
  pub fn fresh(&self) -> Option<&Answer> {
    self.fetched.filter(|at| at.elapsed() < FRESH_FOR).and(self.answer.as_ref())
  }

  pub fn store(&mut self, answer: Answer) {
    self.etag = answer.header("etag");
    self.answer = Some(answer);
    self.fetched = Some(Instant::now());
  }
}

static SLOTS: Mutex<Option<HashMap<String, Arc<Mutex<Slot>>>>> = Mutex::new(None);

/// The slot of `key`. Whoever holds its lock is the one asking the service; an identical read waits on the lock and then finds the answer.
pub(crate) fn slot(key: &str) -> Arc<Mutex<Slot>> {
  let mut slots = lock(&SLOTS);
  let slots = slots.get_or_insert_with(HashMap::new);
  if slots.len() >= MAX_SLOTS && !slots.contains_key(key) {
    slots.retain(|_, slot| Arc::strong_count(slot) > 1);
  }
  slots.entry(key.to_string()).or_default().clone()
}

/// Makes every answer whose key starts with `prefix` stale: the next read asks again, with the `ETag` it has.
pub(crate) fn expire(prefix: &str) {
  let slots: Vec<_> =
    lock(&SLOTS).iter().flat_map(|slots| slots.iter()).filter(|(key, _)| key.starts_with(prefix)).map(|(_, slot)| slot.clone()).collect();
  for slot in slots {
    lock(&slot).fetched = None;
  }
}

/// Forgets every answer whose key starts with `prefix`: something was changed, so what was read may not be true any more.
pub(crate) fn forget(prefix: &str) {
  if let Some(slots) = lock(&SLOTS).as_mut() {
    slots.retain(|key, _| !key.starts_with(prefix));
  }
}

static LIMITS: Mutex<Option<HashMap<String, SystemTime>>> = Mutex::new(None);

/// The host of a URL.
pub(crate) fn host_of(url: &str) -> &str {
  let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
  rest.split(['/', '?']).next().unwrap_or(rest)
}

/// How long `host` still has to be left alone, if it asked for that.
pub(crate) fn blocked(host: &str) -> Option<Duration> {
  let mut limits = lock(&LIMITS);
  let limits = limits.as_mut()?;
  match limits.get(host)?.duration_since(SystemTime::now()) {
    Ok(left) if !left.is_zero() => Some(left),
    _ => {
      limits.remove(host);
      None
    }
  }
}

/// Leaves `host` alone for `wait`.
pub(crate) fn block(host: &str, wait: Duration) {
  lock(&LIMITS).get_or_insert_with(HashMap::new).insert(host.to_string(), SystemTime::now() + wait.min(MAX_WAIT));
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
  headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
}

/// How long a response asks the client to wait, when it is a rate limit (and not a refused token): a `429`, a `Retry-After`, or a
/// limit that is used up (`X-RateLimit-Remaining: 0` with the time of its reset).
pub(crate) fn wait_asked(status: u16, headers: &[(String, String)]) -> Option<Duration> {
  let seconds = |name: &str| header(headers, name).and_then(|value| value.trim().parse::<u64>().ok());
  if let Some(retry) = seconds("retry-after") {
    return Some(Duration::from_secs(retry.max(1)));
  }
  // GitHub says `X-RateLimit-…`, GitLab `RateLimit-…`; both give the reset as a Unix time.
  let used_up = ["x-ratelimit-remaining", "ratelimit-remaining"].iter().any(|name| header(headers, name).is_some_and(|value| value.trim() == "0"));
  if used_up {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let reset =
      seconds("x-ratelimit-reset").or_else(|| seconds("ratelimit-reset")).map_or(DEFAULT_WAIT.as_secs(), |reset| reset.saturating_sub(now).max(1));
    return Some(Duration::from_secs(reset));
  }
  (status == 429).then_some(DEFAULT_WAIT)
}

/// "in about 12 min (at 14:05 UTC)".
pub(crate) fn describe_wait(wait: Duration) -> String {
  let seconds = wait.as_secs().max(1);
  let length = if seconds < 90 { format!("{seconds} s") } else { format!("{} min", seconds.div_ceil(60)) };
  let at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() + seconds;
  format!("in about {length} (at {:02}:{:02} UTC)", at % 86_400 / 3600, at % 3600 / 60)
}

/// What the error of a service that has to be left alone contains.
pub(crate) const RATE_LIMITED: &str = "is limiting requests (its rate limit is used up)";

/// The error for a service that has to be left alone.
pub(crate) fn rate_limited(service: &str, wait: Duration) -> String {
  format!("{service} {RATE_LIMITED}. It can be asked again {}.", describe_wait(wait))
}
