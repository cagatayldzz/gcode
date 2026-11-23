//! Commenting out and in whole lines of text, the way the editor's "Add Line Comment" (⌘K ⌘C), "Remove Line Comment"
//! (⌘K ⌘U) and "Toggle Line Comment" (⌘/) commands do. Works on plain text and byte offsets, so it knows nothing of the editor.

use std::ops::Range;

/// What a command does to the lines it covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommentMode {
  Add,
  Remove,
  /// Removes the comments when every non-blank line has one, else adds them.
  Toggle,
}

/// What goes before and after a commented line, for a language (`name` as `language_for` gives it); none when it has no comments.
pub(crate) fn markers(language: &str) -> Option<(&'static str, &'static str)> {
  match language {
    "rust" | "javascript" | "typescript" | "tsx" | "go" | "swift" | "kotlin" | "c" | "cpp" | "java" => Some(("// ", "")),
    "python" | "bash" | "yaml" | "toml" | "dotenv" | "dockerfile" | "gitignore" | "ruby" | "makefile" => Some(("# ", "")),
    "html" | "markdown" | "xml" => Some(("<!-- ", " -->")),
    "css" | "scss" => Some(("/* ", " */")),
    _ => None,
  }
}

/// The change a command makes: the text of whole lines is replaced, and the selection afterwards.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Edit {
  /// The bytes replaced: from the start of the first line to the end of the last one (its line ending stays).
  pub range: Range<usize>,
  pub text: String,
  /// The selection after the change, in the new text's offsets.
  pub selection: Range<usize>,
}

struct Line<'a> {
  /// Byte offset of the line's start in the whole text.
  start: usize,
  /// The line without its line ending.
  content: &'a str,
  /// The line ending that was cut off (`\r` or nothing; the `\n` is the separator).
  ending: &'a str,
}

fn indent_len(line: &str) -> usize {
  line.len() - line.trim_start_matches([' ', '\t']).len()
}

fn is_blank(line: &str) -> bool {
  line.trim().is_empty()
}

/// The edit that comments or uncomments the lines `selection` touches, or `None` when there is nothing to change. A selection
/// that ends at the very start of a line does not include that line.
pub(crate) fn edit(text: &str, selection: Range<usize>, markers: (&str, &str), mode: CommentMode) -> Option<Edit> {
  let (prefix, suffix) = markers;
  let start = selection.start.min(text.len());
  let end = selection.end.clamp(start, text.len());
  let first = text[..start].rfind('\n').map_or(0, |index| index + 1);
  let last_byte = if end > start && text.as_bytes()[end - 1] == b'\n' { end - 1 } else { end };
  let block_end = text[last_byte..].find('\n').map_or(text.len(), |index| last_byte + index);
  let block = &text[first..block_end];

  let mut lines = Vec::new();
  let mut offset = first;
  for raw in block.split('\n') {
    let (content, ending) = match raw.strip_suffix('\r') {
      Some(content) => (content, "\r"),
      None => (raw, ""),
    };
    lines.push(Line { start: offset, content, ending });
    offset += raw.len() + 1;
  }

  let open = prefix.trim_end();
  let close = suffix.trim_start();
  let has_comment = |line: &str| {
    let body = &line[indent_len(line)..];
    body.starts_with(open) && body.trim_end().ends_with(close) && body.trim_end().len() >= open.len() + close.len()
  };
  let blank_only = lines.iter().all(|line| is_blank(line.content));
  let remove = match mode {
    CommentMode::Add => false,
    CommentMode::Remove => true,
    CommentMode::Toggle => !blank_only && lines.iter().filter(|line| !is_blank(line.content)).all(|line| has_comment(line.content)),
  };
  let common_indent = lines.iter().filter(|line| !is_blank(line.content)).map(|line| indent_len(line.content)).min().unwrap_or(0);

  // For each line: the column the change happens at and how many bytes it adds (or takes away) there, and what is added at the end.
  let mut out = String::new();
  let mut changes: Vec<(usize, usize, isize)> = Vec::new();
  let mut changed = false;
  for (index, line) in lines.iter().enumerate() {
    if index > 0 {
      out.push('\n');
    }
    let content = line.content;
    let blank = is_blank(content);
    if remove {
      if blank || !has_comment(content) {
        out.push_str(content);
        changes.push((line.start, usize::MAX, 0));
      } else {
        let at = indent_len(content);
        let mut body = &content[at..];
        body = &body[open.len()..];
        let mut removed = open.len();
        if let Some(rest) = body.strip_prefix(' ') {
          body = rest;
          removed += 1;
        }
        let mut tail = body.trim_end();
        let trailing = &body[tail.len()..];
        if !close.is_empty() {
          tail = &tail[..tail.len() - close.len()];
          removed += close.len();
          if let Some(rest) = tail.strip_suffix(' ') {
            tail = rest;
            removed += 1;
          }
        }
        out.push_str(&content[..at]);
        out.push_str(tail);
        out.push_str(trailing);
        changes.push((line.start, at, -(removed as isize)));
        changed = true;
      }
    } else if blank && !blank_only {
      out.push_str(content);
      changes.push((line.start, usize::MAX, 0));
    } else {
      let at = if blank_only { indent_len(content) } else { common_indent };
      out.push_str(&content[..at]);
      out.push_str(prefix);
      out.push_str(&content[at..]);
      out.push_str(suffix);
      changes.push((line.start, at, (prefix.len() + suffix.len()) as isize));
      changed = true;
    }
    out.push_str(line.ending);
  }
  if !changed {
    return None;
  }

  // Where an offset of the old text lands in the new one: a column after the change point moves with it.
  let map = |offset: usize| -> usize {
    let offset = offset.clamp(first, block_end);
    let mut new_start = first;
    for (index, (line_start, at, delta)) in changes.iter().enumerate() {
      let line = &lines[index];
      let line_end = line.start + line.content.len() + line.ending.len();
      if offset <= line_end || index == changes.len() - 1 {
        let column = offset - line_start;
        let shifted = if *at == usize::MAX || column <= *at { column as isize } else { (column as isize + delta).max(*at as isize) };
        return new_start + shifted.max(0) as usize;
      }
      let new_len = line.content.len() as isize + if *at == usize::MAX { 0 } else { *delta };
      new_start += new_len as usize + line.ending.len() + 1;
    }
    new_start
  };
  Some(Edit { range: first..block_end, text: out, selection: map(selection.start)..map(selection.end) })
}
