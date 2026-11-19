use std::{fs, path::PathBuf};
use vekro_opencommit::{Config, Provider, DEFAULT_INSTRUCTIONS, DEFAULT_MAX_INPUT_TOKENS, DEFAULT_MODEL};

fn temp_file(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("vekro-opencommit-{name}-{}", std::process::id()));
  let _ = fs::remove_dir_all(&dir);
  fs::create_dir_all(&dir).unwrap();
  dir.join(".opencommit")
}

#[test]
fn a_missing_file_gives_the_defaults() {
  let config = Config::load(&temp_file("missing"));
  assert_eq!(config.provider, Provider::OpenAiCompatible);
  assert_eq!(config.model, DEFAULT_MODEL);
  assert_eq!(config.max_input_tokens, DEFAULT_MAX_INPUT_TOKENS);
  assert_eq!(config.language, "en");
  assert!(config.one_line && config.omit_scope);
  assert!(!config.description && !config.why && !config.emoji && !config.git_push);
  assert!(config.api_key.is_empty());
}

#[test]
fn the_default_instructions_ask_for_a_short_subject() {
  assert!(DEFAULT_INSTRUCTIONS.contains("60 characters"));
}

#[test]
fn values_are_read_from_the_opencommit_file() {
  let path = temp_file("read");
  fs::write(
    &path,
    "# a comment\nOCO_API_KEY=sk-test\nOCO_MODEL='gpt-4o'\nOCO_LANGUAGE=\"tr\"\nOCO_TOKENS_MAX_INPUT=8000\nOCO_ONE_LINE_COMMIT=false\nOCO_GITPUSH=true\nOCO_OMIT_SCOPE=false\n",
  )
  .unwrap();
  let config = Config::load(&path);
  assert_eq!(config.api_key, "sk-test");
  assert_eq!(config.model, "gpt-4o");
  assert_eq!(config.language, "tr");
  assert_eq!(config.max_input_tokens, 8000);
  assert!(!config.one_line && config.git_push && !config.omit_scope);
}

#[test]
fn undefined_and_broken_values_keep_the_defaults() {
  let path = temp_file("broken");
  fs::write(&path, "OCO_API_KEY=undefined\nOCO_TOKENS_MAX_INPUT=many\nOCO_WHY=maybe\n").unwrap();
  let config = Config::load(&path);
  assert!(config.api_key.is_empty());
  assert_eq!(config.max_input_tokens, DEFAULT_MAX_INPUT_TOKENS);
  assert!(!config.why);
}

#[test]
fn saving_keeps_unknown_lines_and_updates_known_ones_in_place() {
  let path = temp_file("save");
  fs::write(&path, "# mine\nOCO_AI_PROVIDER=openai\nOCO_MODEL=old\nOCO_API_KEY=sk-keep\n").unwrap();
  let mut config = Config::load(&path);
  config.model = "gpt-4o".into();
  config.git_push = true;
  config.save(&path).unwrap();
  let text = fs::read_to_string(&path).unwrap();
  let lines: Vec<&str> = text.lines().collect();
  assert_eq!(&lines[..4], ["# mine", "OCO_AI_PROVIDER=openai", "OCO_MODEL=gpt-4o", "OCO_API_KEY=sk-keep"]);
  assert!(text.contains("OCO_GITPUSH=true"));
  assert!(text.contains("OCO_PROMPT_MODULE=conventional-commits"));
  assert_eq!(Config::load(&path), config);
}

#[test]
fn an_empty_key_is_not_written() {
  let path = temp_file("nokey");
  Config::default().save(&path).unwrap();
  assert!(!fs::read_to_string(&path).unwrap().contains("OCO_API_KEY"));
}

#[cfg(unix)]
#[test]
fn the_file_is_private_to_the_user() {
  use std::os::unix::fs::PermissionsExt as _;
  let path = temp_file("mode");
  Config::default().save(&path).unwrap();
  assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn selecting_apple_intelligence_preserves_the_cli_provider_and_api_key() {
  let path = temp_file("provider");
  fs::write(&path, "OCO_AI_PROVIDER=openai\nOCO_API_KEY=sk-keep\n").unwrap();
  let mut config = Config::load(&path);
  config.provider = Provider::AppleIntelligence;
  config.save(&path).unwrap();
  assert_eq!(Config::load(&path), config);
  assert!(fs::read_to_string(&path).unwrap().contains("OCO_AI_PROVIDER=openai\n"));
  assert!(!Config { api_key: String::new(), ..config.clone() }.needs_api_key());

  config.provider = Provider::OpenAiCompatible;
  config.save(&path).unwrap();
  assert_eq!(Config::load(&path), config);
  assert!(!fs::read_to_string(&path).unwrap().contains("VEKRO_COMMIT_PROVIDER"));
  assert!(Config::default().needs_api_key());
}
