use std::{
  fs, io,
  path::{Path, PathBuf},
};

pub const DEFAULT_MODEL: &str = "gpt-4o-mini";
pub const DEFAULT_MAX_INPUT_TOKENS: u32 = 32_000;
pub const DEFAULT_LANGUAGE: &str = "en";

/// What the model is told besides the OpenCommit rules, until the user writes their own.
pub const DEFAULT_INSTRUCTIONS: &str =
  "Write a short subject of at most 60 characters that names only the main change. No filler such as for better user experience.";

/// The models the settings offer; any other model already in the file is kept and shown too.
pub const MODELS: [&str; 5] = ["gpt-4o-mini", "gpt-4o", "gpt-4.1-mini", "gpt-4.1", "o4-mini"];

/// The sizes of diff sent in one request, in tokens.
pub const TOKEN_LIMITS: [u32; 6] = [4_000, 8_000, 16_000, 32_000, 64_000, 128_000];

/// `(code, name)` of the languages the settings offer.
pub const LANGUAGES: [(&str, &str); 10] = [
  ("en", "English"),
  ("tr", "Türkçe"),
  ("de", "Deutsch"),
  ("fr", "Français"),
  ("es", "Español"),
  ("pt", "Português"),
  ("it", "Italiano"),
  ("ru", "Русский"),
  ("ja", "日本語"),
  ("zh_CN", "简体中文"),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Provider {
  #[default]
  OpenAiCompatible,
  AppleIntelligence,
}

/// The OpenCommit settings. Keys this program does not know stay in the file untouched when it is saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
  pub provider: Provider,
  /// The API key; empty when none is set.
  pub api_key: String,
  /// The base URL of an OpenAI-compatible API; empty for OpenAI.
  pub api_url: String,
  pub model: String,
  /// The largest diff sent in one request, in tokens (`OCO_TOKENS_MAX_INPUT`).
  pub max_input_tokens: u32,
  pub language: String,
  /// One line, no body (`OCO_ONE_LINE_COMMIT`).
  pub one_line: bool,
  /// Add a description after the subject (`OCO_DESCRIPTION`).
  pub description: bool,
  /// Explain why the change was made (`OCO_WHY`).
  pub why: bool,
  /// Start the subject with a GitMoji (`OCO_EMOJI`).
  pub emoji: bool,
  /// Leave the scope out of `type(scope):` (`OCO_OMIT_SCOPE`).
  pub omit_scope: bool,
  /// Push after the commit (`OCO_GITPUSH`).
  pub git_push: bool,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      provider: Provider::default(),
      api_key: String::new(),
      api_url: String::new(),
      model: DEFAULT_MODEL.into(),
      max_input_tokens: DEFAULT_MAX_INPUT_TOKENS,
      language: DEFAULT_LANGUAGE.into(),
      one_line: true,
      description: false,
      why: false,
      emoji: false,
      omit_scope: true,
      git_push: false,
    }
  }
}

/// `~/.opencommit`, or the file `VEKRO_OPENCOMMIT_CONFIG` names (for a scratch run that must not touch the real one).
pub fn config_path() -> PathBuf {
  if let Some(path) = std::env::var_os("VEKRO_OPENCOMMIT_CONFIG") {
    return PathBuf::from(path);
  }
  std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".opencommit")
}

fn parse_value(raw: &str) -> &str {
  let value = raw.trim();
  let unquoted =
    value.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).or_else(|| value.strip_prefix('\'').and_then(|rest| rest.strip_suffix('\'')));
  unquoted.unwrap_or(value)
}

fn entries(text: &str) -> impl Iterator<Item = (&str, &str)> {
  text.lines().filter_map(|line| {
    let line = line.trim();
    if line.starts_with('#') {
      return None;
    }
    let (key, value) = line.split_once('=')?;
    Some((key.trim(), parse_value(value)))
  })
}

fn flag(value: &str) -> Option<bool> {
  match value.to_ascii_lowercase().as_str() {
    "true" => Some(true),
    "false" => Some(false),
    _ => None,
  }
}

impl Config {
  pub fn needs_api_key(&self) -> bool {
    self.provider == Provider::OpenAiCompatible && self.api_key.is_empty() && self.api_url.is_empty()
  }

  /// Reads the file; a missing or unreadable one gives the defaults, and a key that is not there keeps its default.
  pub fn load(path: &Path) -> Self {
    let mut config = Self::default();
    let Ok(text) = fs::read_to_string(path) else { return config };
    for (key, value) in entries(&text) {
      let empty = matches!(value, "" | "undefined" | "null");
      match key {
        "VEKRO_COMMIT_PROVIDER" => {
          config.provider = if value == "apple-intelligence" { Provider::AppleIntelligence } else { Provider::default() };
        }
        "OCO_API_KEY" => config.api_key = if empty { String::new() } else { value.into() },
        "OCO_API_URL" => config.api_url = if empty { String::new() } else { value.into() },
        "OCO_MODEL" if !empty => config.model = value.into(),
        "OCO_LANGUAGE" if !empty => config.language = value.into(),
        "OCO_TOKENS_MAX_INPUT" => config.max_input_tokens = value.parse().unwrap_or(config.max_input_tokens),
        "OCO_ONE_LINE_COMMIT" => config.one_line = flag(value).unwrap_or(config.one_line),
        "OCO_DESCRIPTION" => config.description = flag(value).unwrap_or(config.description),
        "OCO_WHY" => config.why = flag(value).unwrap_or(config.why),
        "OCO_EMOJI" => config.emoji = flag(value).unwrap_or(config.emoji),
        "OCO_OMIT_SCOPE" => config.omit_scope = flag(value).unwrap_or(config.omit_scope),
        "OCO_GITPUSH" => config.git_push = flag(value).unwrap_or(config.git_push),
        _ => {}
      }
    }
    config
  }

  fn values(&self) -> Vec<(&'static str, String)> {
    vec![
      (
        "VEKRO_COMMIT_PROVIDER",
        match self.provider {
          Provider::OpenAiCompatible => String::new(),
          Provider::AppleIntelligence => "apple-intelligence".into(),
        },
      ),
      ("OCO_API_KEY", self.api_key.clone()),
      ("OCO_API_URL", self.api_url.clone()),
      ("OCO_MODEL", self.model.clone()),
      ("OCO_LANGUAGE", self.language.clone()),
      ("OCO_TOKENS_MAX_INPUT", self.max_input_tokens.to_string()),
      ("OCO_ONE_LINE_COMMIT", self.one_line.to_string()),
      ("OCO_DESCRIPTION", self.description.to_string()),
      ("OCO_WHY", self.why.to_string()),
      ("OCO_EMOJI", self.emoji.to_string()),
      ("OCO_OMIT_SCOPE", self.omit_scope.to_string()),
      ("OCO_GITPUSH", self.git_push.to_string()),
      ("OCO_PROMPT_MODULE", "conventional-commits".into()),
    ]
  }

  /// Writes the settings, keeping every other line of the file as it was. The file is private to the user (it holds the key).
  pub fn save(&self, path: &Path) -> io::Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    let values = self.values();
    let mut written = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for line in existing.lines() {
      let key = line.split_once('=').map(|(key, _)| key.trim()).filter(|_| !line.trim_start().starts_with('#'));
      match key.and_then(|key| values.iter().find(|(name, _)| *name == key)) {
        Some((name, value)) => {
          // An empty URL or key is dropped, not written as an empty value.
          if !value.is_empty() {
            lines.push(format!("{name}={value}"));
          }
          written.push(*name);
        }
        None => lines.push(line.to_string()),
      }
    }
    for (name, value) in &values {
      if !written.contains(name) && !value.is_empty() {
        lines.push(format!("{name}={value}"));
      }
    }
    let temporary = path.with_extension("vekro-tmp");
    fs::write(&temporary, lines.join("\n") + "\n")?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temporary, path)
  }
}
