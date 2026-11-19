use crate::{estimate_tokens, parse_message, split_diff, system_prompt, Config, Message, Provider};
use serde_json::{json, Value};
use std::time::Duration;

/// The tokens kept free for the instructions and the answer in each request.
const RESERVED_TOKENS: usize = 1_000;

fn endpoint(config: &Config) -> String {
  let base = config.api_url.trim().trim_end_matches('/');
  match base {
    "" => "https://api.openai.com/v1/chat/completions".into(),
    base if base.ends_with("/chat/completions") => base.into(),
    base => format!("{base}/chat/completions"),
  }
}

/// One request to the chat API; the answer's text.
fn complete(config: &Config, system: &str, user: &str) -> Result<String, String> {
  let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(90))).http_status_as_error(false).build().into();
  let body = json!({
    "model": config.model,
    "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
  });
  let mut request = agent.post(&endpoint(config));
  if !config.api_key.is_empty() {
    request = request.header("Authorization", format!("Bearer {}", config.api_key));
  }
  let mut response = request.send_json(&body).map_err(|error| format!("Could not reach the commit message service: {error}"))?;
  let status = response.status().as_u16();
  let text = response.body_mut().read_to_string().map_err(|error| format!("Could not read the answer: {error}"))?;
  let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
  if status != 200 {
    let detail = value["error"]["message"].as_str().unwrap_or("");
    return Err(match status {
      401 => "The API key was rejected. Check it in the OpenCommit settings.".into(),
      429 => format!("The service is rate limiting requests or the quota is used up. {detail}").trim().into(),
      _ => format!("The service answered with HTTP {status}. {detail}").trim().into(),
    });
  }
  value["choices"][0]["message"]["content"]
    .as_str()
    .map(str::to_owned)
    .filter(|content| !content.trim().is_empty())
    .ok_or_else(|| "The service sent no commit message.".into())
}

/// Writes a commit message for `diff` (a unified diff). A diff larger than the configured limit is split per file, each part
/// is summarized, and the summaries are merged into one message in a last request. Blocking: call it off the interface thread.
pub fn generate(config: &Config, instructions: &str, diff: &str) -> Result<Message, String> {
  if diff.trim().is_empty() {
    return Err("There are no changes to describe.".into());
  }
  if config.needs_api_key() {
    return Err("Add your API key in the OpenCommit settings first.".into());
  }
  let system = system_prompt(config, instructions);
  let answer = match config.provider {
    Provider::OpenAiCompatible => generate_openai(config, &system, diff)?,
    Provider::AppleIntelligence => {
      #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
      {
        crate::apple::generate(&system, diff)?
      }
      #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
      {
        return Err("Apple Intelligence requires an Apple silicon Mac with macOS 26 or later.".into());
      }
    }
  };
  let message = parse_message(&answer, config.one_line);
  if message.summary.is_empty() {
    return Err("The service sent no commit message.".into());
  }
  Ok(message)
}

fn generate_openai(config: &Config, system: &str, diff: &str) -> Result<String, String> {
  let budget = (config.max_input_tokens as usize).saturating_sub(estimate_tokens(system) + RESERVED_TOKENS).max(RESERVED_TOKENS);
  let parts = split_diff(diff, budget);
  if let [only] = parts.as_slice() {
    complete(config, system, only)
  } else {
    let mut summaries = Vec::new();
    for part in &parts {
      summaries.push(complete(config, system, part)?);
    }
    let merge = format!("These commit messages describe parts of one change. Merge them into a single commit message:\n\n{}", summaries.join("\n\n"));
    complete(config, system, &merge)
  }
}
