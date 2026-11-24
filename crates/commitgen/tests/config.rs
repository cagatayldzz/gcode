use std::{fs, path::PathBuf};
use ws_commitgen::{Config, Provider, DEFAULT_INSTRUCTIONS, DEFAULT_MAX_INPUT_TOKENS, DEFAULT_MODEL};

fn temp_dir(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("ws-commitgen-{name}-{}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  dir
}

#[test]
fn a_missing_file_gives_the_defaults() {
  let config = Config::load(&temp_dir("missing").join("commit.json"));
  assert_eq!(config.provider, Provider::OpenAiCompatible);
  assert_eq!(config.model, DEFAULT_MODEL);
  assert_eq!(config.max_input_tokens, DEFAULT_MAX_INPUT_TOKENS);
  assert_eq!(config.language, "en");
  assert!(config.one_line && config.omit_scope);
  assert!(!config.description && !config.why && !config.emoji && !config.git_push);
  assert!(config.api_key.is_empty());
  assert!(config.needs_api_key());
}

#[test]
fn the_default_instructions_ask_for_a_short_subject() {
  assert!(DEFAULT_INSTRUCTIONS.contains("60 characters"));
}

#[test]
fn saved_settings_load_back_and_missing_keys_keep_defaults() {
  let path = temp_dir("roundtrip").join("nested").join("commit.json");
  let config = Config {
    api_key: "sk-test".into(),
    model: "gpt-4o".into(),
    language: "tr".into(),
    max_input_tokens: 8000,
    one_line: false,
    git_push: true,
    provider: Provider::AppleIntelligence,
    ..Config::default()
  };
  config.save(&path).unwrap();
  assert_eq!(Config::load(&path), config);

  fs::write(&path, r#"{"model": "gpt-4.1", "max_input_tokens": "many", "why": "maybe"}"#).unwrap();
  let partial = Config::load(&path);
  assert_eq!(partial.model, "gpt-4.1");
  assert_eq!(partial.max_input_tokens, DEFAULT_MAX_INPUT_TOKENS);
  assert!(!partial.why);
}

#[test]
fn a_broken_file_gives_the_defaults() {
  let path = temp_dir("broken").join("commit.json");
  fs::write(&path, "not json").unwrap();
  assert_eq!(Config::load(&path), Config::default());
}

#[cfg(unix)]
#[test]
fn the_file_is_private_to_the_user() {
  use std::os::unix::fs::PermissionsExt as _;
  let path = temp_dir("mode").join("commit.json");
  Config::default().save(&path).unwrap();
  assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn a_dotfile_from_an_earlier_setup_can_be_imported() {
  let path = temp_dir("import").join("old");
  assert!(Config::import_dotfile(&path).is_none());
  fs::write(&path, "# note\nOCO_API_KEY=sk-keep\nOCO_MODEL='gpt-4o'\nOCO_LANGUAGE=\"tr\"\nOCO_TOKENS_MAX_INPUT=8000\nOCO_ONE_LINE_COMMIT=false\nOCO_GITPUSH=true\nOCO_API_URL=undefined\nOCO_WHY=maybe\n").unwrap();
  let config = Config::import_dotfile(&path).unwrap();
  assert_eq!((config.api_key.as_str(), config.model.as_str(), config.language.as_str()), ("sk-keep", "gpt-4o", "tr"));
  assert_eq!(config.max_input_tokens, 8000);
  assert!(!config.one_line && config.git_push && !config.why);
  assert!(config.api_url.is_empty());
}
