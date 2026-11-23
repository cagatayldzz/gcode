//! Highlighting of the line-based languages the syntax highlighter has no grammar for (`.env`, `.gitignore`).
//!
//! The whole text is classified once into [`Highlights`]: ordered runs that cover every byte, newlines included, which is what
//! an editor lays its text out from. Drawing then only slices the runs ([`Highlights::slice`]) instead of reading the text again.

use std::ops::Range;

/// A language [`Highlights`] can be computed for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
  /// `KEY=value` lines and `#` comments.
  Dotenv,
  /// Patterns, with `#` at the start of a line for a comment.
  Gitignore,
  /// Instructions (`FROM`, `RUN`, ...), variables and `#` comments; a line ending in `\` continues on the next one.
  Dockerfile,
}

impl Language {
  /// The language of a file name, if it is one of these: `.gitignore`, `.env`, `.env.*`, `*.env`, `Dockerfile`, `Dockerfile.*`,
  /// `*.Dockerfile` and `Containerfile`.
  pub fn for_file_name(name: &str) -> Option<Self> {
    let lower = name.to_ascii_lowercase();
    if name == ".gitignore" {
      Some(Self::Gitignore)
    } else if name == ".env" || name.starts_with(".env.") || name.ends_with(".env") {
      Some(Self::Dotenv)
    } else if lower == "dockerfile" || lower == "containerfile" || lower.starts_with("dockerfile.") || lower.ends_with(".dockerfile") {
      Some(Self::Dockerfile)
    } else {
      None
    }
  }

  /// The name the editor knows the language by.
  pub fn name(self) -> &'static str {
    match self {
      Self::Dotenv => "dotenv",
      Self::Gitignore => "gitignore",
      Self::Dockerfile => "dockerfile",
    }
  }

  pub fn from_name(name: &str) -> Option<Self> {
    match name {
      "dotenv" => Some(Self::Dotenv),
      "gitignore" => Some(Self::Gitignore),
      "dockerfile" => Some(Self::Dockerfile),
      _ => None,
    }
  }
}

/// What a run of text is, which the interface turns into a color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
  Comment,
  /// The name before the `=` of a setting.
  Name,
  /// `export`.
  Keyword,
  /// A web address (underlined).
  Link,
  /// A web address in a comment: the comment's color, underlined.
  CommentLink,
  /// A Dockerfile instruction (`FROM`, `RUN`, ...) and the `AS` of a build stage.
  Instruction,
  /// `$NAME` or `${NAME}`.
  Variable,
  /// The value of a setting and a quoted string of a Dockerfile: drawn as a string.
  Value,
  /// A file or folder name of a `.gitignore` pattern.
  Entity,
  /// The characters of a `[...]` class of a `.gitignore` pattern.
  Constant,
}

/// A stretch of the text and what it is; `None` is plain text.
pub type Run = (Range<usize>, Option<Role>);

/// Texts longer than this are not classified: they are not configuration files, and the work is linear in the length.
pub const MAX_BYTES: usize = 1024 * 1024;

/// The runs of one text: ordered, not overlapping, with no gap from byte 0 to the end of the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Highlights {
  runs: Vec<Run>,
  length: usize,
}

impl Highlights {
  /// Classifies `text`. A text over [`MAX_BYTES`] is left plain.
  pub fn compute(language: Language, text: &str) -> Self {
    let mut builder = Builder::default();
    if text.len() <= MAX_BYTES {
      let mut offset = 0;
      let mut continued = false;
      for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        match language {
          Language::Dotenv => dotenv_line(body, offset, &mut builder),
          Language::Gitignore => gitignore_line(body, offset, &mut builder),
          Language::Dockerfile => continued = dockerfile_line(body, offset, continued, &mut builder),
        }
        offset += line.len();
      }
    }
    builder.finish(text.len())
  }

  /// The length of the text these were computed for.
  pub fn text_len(&self) -> usize {
    self.length
  }

  pub fn runs(&self) -> &[Run] {
    &self.runs
  }

  /// The runs inside `range`, cut at its edges. They cover `range` completely when it lies inside the text; a part beyond the end
  /// of the text is plain. Finding the first run is a binary search, so drawing a screenful costs the same in a long file.
  pub fn slice(&self, range: &Range<usize>) -> Vec<Run> {
    let mut out = Vec::new();
    let first = self.runs.partition_point(|(run, _)| run.end <= range.start);
    let mut cursor = range.start;
    for (run, role) in &self.runs[first..] {
      if run.start >= range.end {
        break;
      }
      let (start, end) = (run.start.max(cursor), run.end.min(range.end));
      if start > cursor {
        out.push((cursor..start, None));
      }
      if start < end {
        out.push((start..end, *role));
        cursor = end;
      }
    }
    if cursor < range.end {
      out.push((cursor..range.end, None));
    }
    out
  }
}

/// Collects styled spans in text order and fills the plain text between them.
#[derive(Default)]
struct Builder {
  runs: Vec<Run>,
  end: usize,
}

impl Builder {
  fn push(&mut self, span: Range<usize>, role: Role) {
    if span.start >= span.end || span.start < self.end {
      return;
    }
    self.plain_until(span.start);
    self.end = span.end;
    self.runs.push((span, Some(role)));
  }

  fn plain_until(&mut self, offset: usize) {
    if offset > self.end {
      match self.runs.last_mut() {
        Some((run, None)) => run.end = offset,
        _ => self.runs.push((self.end..offset, None)),
      }
      self.end = offset;
    }
  }

  fn finish(mut self, length: usize) -> Highlights {
    self.plain_until(length);
    Highlights { runs: self.runs, length }
  }
}

fn gitignore_line(line: &str, offset: usize, builder: &mut Builder) {
  // Git treats a hash as a comment only at the very start of a line.
  if line.starts_with('#') {
    builder.push(offset..offset + line.len(), Role::Comment);
    return;
  }
  // The wildcards and a leading `!` are keywords, the characters of a `[...]` class are constants, a backslash and the character it
  // escapes are plain, and the names between them are entities.
  let mut start = 0;
  let mut class_start = None;
  let mut characters = line.char_indices().peekable();
  while let Some((at, character)) = characters.next() {
    if let Some(from) = class_start {
      if character == ']' {
        builder.push(offset + from..offset + at, Role::Constant);
        class_start = None;
        start = at + 1;
      }
      continue;
    }
    let wildcard = matches!(character, '*' | '?') || (character == '!' && at == 0);
    if wildcard || matches!(character, '/' | '[' | ']' | '\\') || character.is_whitespace() {
      builder.push(offset + start..offset + at, Role::Entity);
      if wildcard {
        builder.push(offset + at..offset + at + 1, Role::Keyword);
      }
      start = at + character.len_utf8();
      if character == '[' && line[start..].contains(']') {
        class_start = Some(start);
      } else if character == '\\' {
        // The escaped character is plain as well.
        if let Some((_, escaped)) = characters.next() {
          start += escaped.len_utf8();
        }
      }
    }
  }
  if class_start.is_none() {
    builder.push(offset + start..offset + line.len(), Role::Entity);
  }
}

/// The instructions a Dockerfile line can start with.
const INSTRUCTIONS: [&str; 19] = [
  "FROM",
  "RUN",
  "CMD",
  "LABEL",
  "MAINTAINER",
  "EXPOSE",
  "ENV",
  "ADD",
  "COPY",
  "ENTRYPOINT",
  "VOLUME",
  "USER",
  "WORKDIR",
  "ARG",
  "ONBUILD",
  "STOPSIGNAL",
  "HEALTHCHECK",
  "SHELL",
  "CHECKPOINT",
];

/// One Dockerfile line. `continued` tells that the line before ended in a backslash, so this one is the rest of an instruction and
/// starts with no instruction of its own. Returns whether the next line continues this one.
fn dockerfile_line(line: &str, offset: usize, continued: bool, builder: &mut Builder) -> bool {
  let indent = line.len() - line.trim_start().len();
  let body = &line[indent..];
  // A comment line inside a continued instruction does not end it.
  if body.starts_with('#') {
    builder.push(offset + indent..offset + line.len(), Role::Comment);
    return continued;
  }
  let mut spans: Vec<(Range<usize>, Role)> = Vec::new();
  let mut from = indent;
  if !continued {
    // `ONBUILD` carries another instruction after it.
    for _ in 0..2 {
      let rest = &line[from..];
      let skipped = rest.len() - rest.trim_start().len();
      let word_start = from + skipped;
      let word = line[word_start..].split(char::is_whitespace).next().unwrap_or_default();
      if !INSTRUCTIONS.iter().any(|instruction| instruction.eq_ignore_ascii_case(word)) {
        break;
      }
      spans.push((word_start..word_start + word.len(), Role::Instruction));
      from = word_start + word.len();
      if !word.eq_ignore_ascii_case("ONBUILD") {
        break;
      }
    }
    // The name of a build stage: `FROM image AS name`.
    if line[indent..].split_whitespace().next().is_some_and(|word| word.eq_ignore_ascii_case("FROM")) {
      let mut position = from;
      for word in line[from..].split_inclusive(char::is_whitespace) {
        let trimmed = word.trim_end();
        if trimmed.eq_ignore_ascii_case("AS") {
          spans.push((position..position + trimmed.len(), Role::Instruction));
        }
        position += word.len();
      }
    }
  }
  spans.extend(quoted(line).into_iter().map(|span| (span, Role::Value)));
  spans.extend(variables(line).into_iter().map(|span| (span, Role::Variable)));
  spans.sort_by_key(|(span, _)| span.start);
  for (span, role) in spans {
    builder.push(offset + span.start..offset + span.end, role);
  }
  line.trim_end().ends_with('\\')
}

/// The quoted strings of a line, quotes included. A quote with no partner on the line starts no string.
fn quoted(line: &str) -> Vec<Range<usize>> {
  let mut found = Vec::new();
  let mut open: Option<(usize, char)> = None;
  let mut escaped = false;
  for (at, character) in line.char_indices() {
    if escaped {
      escaped = false;
      continue;
    }
    match (open, character) {
      (_, '\\') => escaped = true,
      (None, '"' | '\'') => open = Some((at, character)),
      (Some((start, quote)), _) if character == quote => {
        found.push(start..at + 1);
        open = None;
      }
      _ => {}
    }
  }
  found
}

/// `$NAME` and `${NAME...}` in a line.
fn variables(line: &str) -> Vec<Range<usize>> {
  let bytes = line.as_bytes();
  let mut found = Vec::new();
  let mut at = 0;
  while at < bytes.len() {
    if bytes[at] == b'\\' {
      // An escaped character is not the start of a variable.
      at += 2;
      continue;
    }
    if bytes[at] != b'$' {
      at += 1;
      continue;
    }
    let end = match bytes.get(at + 1) {
      Some(b'{') => line[at..].find('}').map(|close| at + close + 1),
      Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_' => {
        let length = bytes[at + 1..].iter().take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_').count();
        Some(at + 1 + length)
      }
      _ => None,
    };
    match end {
      Some(end) => {
        found.push(at..end);
        at = end;
      }
      None => at += 1,
    }
  }
  found
}

fn dotenv_line(line: &str, offset: usize, builder: &mut Builder) {
  let indent = line.len() - line.trim_start().len();
  let body = &line[indent..];
  let addresses = addresses(line);
  if body.starts_with('#') {
    // The comment's color around the addresses, which keep it under their underline.
    let mut cursor = indent;
    for address in &addresses {
      builder.push(offset + cursor..offset + address.start, Role::Comment);
      builder.push(offset + address.start..offset + address.end, Role::CommentLink);
      cursor = address.end;
    }
    builder.push(offset + cursor..offset + line.len(), Role::Comment);
    return;
  }
  let (export, rest) = match body.strip_prefix("export") {
    Some(rest) if rest.starts_with([' ', '\t']) => ("export".len(), rest.trim_start()),
    _ => (0, body),
  };
  let name_start = line.len() - rest.len();
  if export > 0 {
    builder.push(offset + indent..offset + indent + export, Role::Keyword);
  }
  let mut value_from = None;
  if let Some(equals) = rest.find('=') {
    let name = &rest[..equals];
    if !name.is_empty() && name.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')) {
      builder.push(offset + name_start..offset + name_start + equals, Role::Name);
      builder.push(offset + name_start + equals..offset + name_start + equals + 1, Role::Keyword);
      value_from = Some(name_start + equals + 1);
    }
  }
  // The value is a string, up to a closing quote or a ` #` that starts a trailing comment; the web addresses in it keep their own
  // role. A line that is no setting only has its addresses marked.
  let mut cursor = value_from.unwrap_or(0);
  let value_end = value_from.map_or(line.len(), |from| value_end(line, from));
  for address in addresses {
    if address.start >= cursor && address.end <= value_end {
      if value_from.is_some() {
        builder.push(offset + cursor..offset + address.start, Role::Value);
      }
      builder.push(offset + address.start..offset + address.end, Role::Link);
      cursor = address.end;
    }
  }
  if value_from.is_some() {
    builder.push(offset + cursor..offset + value_end, Role::Value);
    if let Some(hash) = line[value_end..].find('#') {
      builder.push(offset + value_end + hash..offset + line.len(), Role::Comment);
    }
  }
}

/// Where the value that starts at `from` ends: after its closing quote, else before a `#` that follows a space.
fn value_end(line: &str, from: usize) -> usize {
  let value = &line[from..];
  match value.chars().next() {
    Some(quote @ ('"' | '\'')) => {
      let mut escaped = false;
      for (at, character) in value.char_indices().skip(1) {
        match character {
          _ if escaped => escaped = false,
          '\\' if quote == '"' => escaped = true,
          _ if character == quote => return from + at + 1,
          _ => {}
        }
      }
      line.len()
    }
    _ => value.find([' ', '\t']).and_then(|space| value[space..].trim_start().starts_with('#').then_some(from + space)).unwrap_or(line.len()),
  }
}

/// The web addresses of a line, each up to the next space.
fn addresses(line: &str) -> Vec<Range<usize>> {
  let mut found = Vec::new();
  let mut from = 0;
  while let Some(offset) = [line[from..].find("http://"), line[from..].find("https://")].into_iter().flatten().min() {
    let start = from + offset;
    let end = line[start..].find(char::is_whitespace).map_or(line.len(), |length| start + length);
    found.push(start..end);
    from = end;
  }
  found
}
