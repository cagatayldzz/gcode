use serde_json::{json, Map, Value};
use std::{
  fs, io,
  path::{Path, PathBuf},
};

pub const DEFAULT_MODEL: &str = "gpt-4o-mini";
pub const DEFAULT_MAX_INPUT_TOKENS: u32 = 32_000;
pub const DEFAULT_LANGUAGE: &str = "en";

/// What the model is told besides the built-in rules, until the user writes their own.
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

/// The commit message settings, kept in one JSON file in the data directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
  pub provider: Provider,
  /// The API key; empty when none is set.
  pub api_key: String,
  /// The base URL of an OpenAI-compatible API; empty for OpenAI.
  pub api_url: String,
  pub model: String,
  /// The largest diff sent in one request, in tokens.
  pub max_input_tokens: u32,
  pub language: String,
  /// One line, no body.
  pub one_line: bool,
  /// Add a description after the subject.
  pub description: bool,
  /// Explain why the change was made.
  pub why: bool,
  /// Start the subject with a GitMoji.
  pub emoji: bool,
  /// Leave the scope out of `type(scope):`.
  pub omit_scope: bool,
  /// Push after the commit.
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

/// `commit.json` in the data directory.
pub fn config_path() -> PathBuf {
  ws_storage::data_dir().join("commit.json")
}

fn text(map: &Map<String, Value>, key: &str) -> Option<String> {
  map.get(key).and_then(Value::as_str).filter(|value| !value.is_empty()).map(str::to_owned)
}

fn flag(map: &Map<String, Value>, key: &str, default: bool) -> bool {
  map.get(key).and_then(Value::as_bool).unwrap_or(default)
}

impl Config {
  pub fn needs_api_key(&self) -> bool {
    self.provider == Provider::OpenAiCompatible && self.api_key.is_empty() && self.api_url.is_empty()
  }

  /// Reads the file; a missing or unreadable one gives the defaults, and a key that is not there keeps its default.
  pub fn load(path: &Path) -> Self {
    let mut config = Self::default();
    let Some(Value::Object(map)) = fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()) else { return config };
    if text(&map, "provider").as_deref() == Some("apple-intelligence") {
      config.provider = Provider::AppleIntelligence;
    }
    config.api_key = text(&map, "api_key").unwrap_or_default();
    config.api_url = text(&map, "api_url").unwrap_or_default();
    config.model = text(&map, "model").unwrap_or(config.model);
    config.language = text(&map, "language").unwrap_or(config.language);
    config.max_input_tokens =
      map.get("max_input_tokens").and_then(Value::as_u64).and_then(|value| u32::try_from(value).ok()).unwrap_or(config.max_input_tokens);
    config.one_line = flag(&map, "one_line", config.one_line);
    config.description = flag(&map, "description", config.description);
    config.why = flag(&map, "why", config.why);
    config.emoji = flag(&map, "emoji", config.emoji);
    config.omit_scope = flag(&map, "omit_scope", config.omit_scope);
    config.git_push = flag(&map, "git_push", config.git_push);
    config
  }

  /// Loads `commit.json`. The first time (no file yet), takes over the settings of an earlier setup's `~/.opencommit` file and saves them.
  pub fn load_or_import() -> Self {
    let path = config_path();
    if path.exists() {
      return Self::load(&path);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    match Self::import_dotfile(&home.join(".opencommit")) {
      Some(config) => {
        let _ = config.save(&path);
        config
      }
      None => Self::default(),
    }
  }

  /// Reads the settings an earlier setup kept in a `KEY=value` dotfile (the `~/.opencommit` file), so an API key already there is not lost.
  pub fn import_dotfile(path: &Path) -> Option<Self> {
    let contents = fs::read_to_string(path).ok()?;
    let mut config = Self::default();
    for line in contents.lines().map(str::trim).filter(|line| !line.starts_with('#')) {
      let Some((key, value)) = line.split_once('=') else { continue };
      let value = value.trim().trim_matches(|character| character == '"' || character == '\'');
      let empty = matches!(value, "" | "undefined" | "null");
      let flag = |current: bool| value.parse().unwrap_or(current);
      match key.trim() {
        "OCO_API_KEY" if !empty => config.api_key = value.into(),
        "OCO_API_URL" if !empty => config.api_url = value.into(),
        "OCO_MODEL" if !empty => config.model = value.into(),
        "OCO_LANGUAGE" if !empty => config.language = value.into(),
        "OCO_TOKENS_MAX_INPUT" => config.max_input_tokens = value.parse().unwrap_or(config.max_input_tokens),
        "OCO_ONE_LINE_COMMIT" => config.one_line = flag(config.one_line),
        "OCO_DESCRIPTION" => config.description = flag(config.description),
        "OCO_WHY" => config.why = flag(config.why),
        "OCO_EMOJI" => config.emoji = flag(config.emoji),
        "OCO_OMIT_SCOPE" => config.omit_scope = flag(config.omit_scope),
        "OCO_GITPUSH" => config.git_push = flag(config.git_push),
        _ => {}
      }
    }
    Some(config)
  }

  /// Writes the settings. The file is private to the user (it holds the key).
  pub fn save(&self, path: &Path) -> io::Result<()> {
    let value = json!({
      "provider": match self.provider {
        Provider::OpenAiCompatible => "openai-compatible",
        Provider::AppleIntelligence => "apple-intelligence",
      },
      "api_key": self.api_key,
      "api_url": self.api_url,
      "model": self.model,
      "language": self.language,
      "max_input_tokens": self.max_input_tokens,
      "one_line": self.one_line,
      "description": self.description,
      "why": self.why,
      "emoji": self.emoji,
      "omit_scope": self.omit_scope,
      "git_push": self.git_push,
    });
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_string_pretty(&value).map_err(io::Error::other)? + "\n")?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temporary, path)
  }
}
