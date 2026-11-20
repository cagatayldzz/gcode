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
}

impl Language {
  /// The language of a file name, if it is one of these: `.gitignore`, `.env`, `.env.*` and `*.env`.
  pub fn for_file_name(name: &str) -> Option<Self> {
    if name == ".gitignore" {
      Some(Self::Gitignore)
    } else if name == ".env" || name.starts_with(".env.") || name.ends_with(".env") {
      Some(Self::Dotenv)
    } else {
      None
    }
  }

  /// The name the editor knows the language by.
  pub fn name(self) -> &'static str {
    match self {
      Self::Dotenv => "dotenv",
      Self::Gitignore => "gitignore",
    }
  }

  pub fn from_name(name: &str) -> Option<Self> {
    match name {
      "dotenv" => Some(Self::Dotenv),
      "gitignore" => Some(Self::Gitignore),
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
      for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        match language {
          Language::Dotenv => dotenv_line(body, offset, &mut builder),
          Language::Gitignore => gitignore_line(body, offset, &mut builder),
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
  }
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
  if let Some(equals) = rest.find('=') {
    let name = &rest[..equals];
    if !name.is_empty() && name.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')) {
      builder.push(offset + name_start..offset + name_start + equals, Role::Name);
    }
  }
  for address in addresses {
    builder.push(offset + address.start..offset + address.end, Role::Link);
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
