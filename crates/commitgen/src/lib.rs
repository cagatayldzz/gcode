//! Commit message generation for the app.
//!
//! The settings live in `commit.json` in the data directory. Messages use an OpenAI-compatible chat API or Apple Intelligence.
//! Never depends on the interface.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod apple;
mod client;
mod config;
mod prompt;

pub use client::generate;
pub use config::{
  config_path, Config, Provider, DEFAULT_INSTRUCTIONS, DEFAULT_LANGUAGE, DEFAULT_MAX_INPUT_TOKENS, DEFAULT_MODEL, LANGUAGES, MODELS, TOKEN_LIMITS,
};
pub use prompt::{estimate_tokens, parse_message, shorten_files, split_diff, system_prompt, Message};
