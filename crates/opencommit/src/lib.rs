//! Commit message generation for Vekro, compatible with [OpenCommit](https://github.com/di-sukharev/opencommit).
//!
//! The settings live in the same `~/.opencommit` file the `oco` command uses, so an API key that is already there keeps
//! working. Messages use an OpenAI-compatible chat API or Apple Intelligence. Never depends on the interface.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod apple;
mod client;
mod config;
mod prompt;

pub use client::generate;
pub use config::{
  config_path, Config, Provider, DEFAULT_INSTRUCTIONS, DEFAULT_LANGUAGE, DEFAULT_MAX_INPUT_TOKENS, DEFAULT_MODEL, LANGUAGES, MODELS, TOKEN_LIMITS,
};
pub use prompt::{estimate_tokens, parse_message, split_diff, system_prompt, Message};
