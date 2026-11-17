use crate::Config;

/// The commit message split into what goes in the summary field and what goes in the description field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
  pub summary: String,
  pub description: String,
}

/// The rules the model follows: the Conventional Commits convention of OpenCommit, shaped by the settings, then the
/// user's own instructions.
pub fn system_prompt(config: &Config, instructions: &str) -> String {
  let mut prompt = String::from(
    "You are to act as the author of a commit message in git. Your mission is to create clean and comprehensive commit messages \
     in the Conventional Commits convention, such as `feat: add search` or `fix(ui): keep the cursor visible`. \
     I will send you the output of `git diff` and you are to convert it into a commit message. \
     Use the present tense and the imperative mood. Do not add any text besides the commit message, no quotes and no code fences.",
  );
  if config.omit_scope {
    prompt.push_str(" Do not put a scope in parentheses after the type.");
  }
  if config.emoji {
    prompt.push_str(" Start the subject with a GitMoji that fits the change, in the form `✨ feat: ...`.");
  }
  if config.one_line {
    prompt.push_str(
      " Write one single line: a concise commit message that covers all changes with an emphasis on the primary update. \
       Do not write a body and do not list a commit per file.",
    );
  } else {
    prompt.push_str(" Write a subject line, then an empty line and a short body. The subject must not be longer than 72 characters.");
    if config.description {
      prompt.push_str(" The body describes what changed in a few short lines.");
    }
  }
  if config.why && !config.one_line {
    prompt.push_str(" Explain why the change was made, not only what changed.");
  } else if config.why {
    prompt.push_str(" Let the subject say why the change was made when the diff makes that clear.");
  }
  let language = crate::LANGUAGES.iter().find(|(code, _)| *code == config.language).map_or(config.language.as_str(), |(_, name)| *name);
  prompt.push_str(&format!(" Write the commit message in {language}."));
  let instructions = instructions.trim();
  if !instructions.is_empty() {
    prompt.push_str(&format!("\n\nAdditional instructions from the user, which win over the rules above:\n{instructions}"));
  }
  prompt
}

/// A cheap estimate of the tokens in `text`: about four characters each. A real tokenizer would be slower than the request.
pub fn estimate_tokens(text: &str) -> usize {
  text.chars().count().div_ceil(4)
}

/// Cuts a unified diff into parts of at most `max_tokens` (estimated), per file: files are kept whole and grouped while they
/// fit; a file that alone is too large is cut at a line and marked as truncated.
pub fn split_diff(diff: &str, max_tokens: usize) -> Vec<String> {
  let max_tokens = max_tokens.max(1);
  let mut files: Vec<String> = Vec::new();
  for line in diff.split_inclusive('\n') {
    if line.starts_with("diff --git ") || files.is_empty() {
      files.push(String::new());
    }
    if let Some(file) = files.last_mut() {
      file.push_str(line);
    }
  }
  let mut parts: Vec<String> = Vec::new();
  let mut current = String::new();
  for file in files.into_iter().filter(|file| !file.trim().is_empty()) {
    let file = if estimate_tokens(&file) > max_tokens { truncate(&file, max_tokens) } else { file };
    if !current.is_empty() && estimate_tokens(&current) + estimate_tokens(&file) > max_tokens {
      parts.push(std::mem::take(&mut current));
    }
    current.push_str(&file);
  }
  if !current.is_empty() {
    parts.push(current);
  }
  parts
}

fn truncate(file: &str, max_tokens: usize) -> String {
  let mut kept = String::new();
  for line in file.split_inclusive('\n') {
    if estimate_tokens(&kept) + estimate_tokens(line) + 8 > max_tokens {
      break;
    }
    kept.push_str(line);
  }
  if !kept.ends_with('\n') {
    kept.push('\n');
  }
  kept.push_str("[diff truncated]\n");
  kept
}

/// Turns the model's answer into the two fields: the first line is the summary, what follows an empty line the description.
/// Code fences and quotes around the answer are removed. With `one_line` the description is dropped.
pub fn parse_message(raw: &str, one_line: bool) -> Message {
  let mut text = raw.trim();
  if let Some(rest) = text.strip_prefix("```") {
    let rest = rest.split_once('\n').map_or("", |(_, rest)| rest);
    text = rest.trim_end().strip_suffix("```").unwrap_or(rest).trim();
  }
  let text = text.trim_matches(|character| character == '"' || character == '`').trim();
  let (summary, description) = text.split_once('\n').unwrap_or((text, ""));
  Message { summary: summary.trim().to_string(), description: if one_line { String::new() } else { description.trim().to_string() } }
}
