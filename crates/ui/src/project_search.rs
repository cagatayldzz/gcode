//! Search and replace project text on the background executor.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::{Captures, Regex, RegexBuilder};
use std::{
  fs,
  io::Write,
  ops::Range,
  path::{Path, PathBuf},
  sync::atomic::{AtomicBool, Ordering},
};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_REPLACEMENT_BYTES: usize = 64 * 1024 * 1024;
const MAX_RESULTS: usize = 10_000;

#[derive(Clone, Default)]
pub(crate) struct Options {
  pub query: String,
  pub case_sensitive: bool,
  pub whole_word: bool,
  pub regex: bool,
  pub include: String,
  pub exclude: String,
}

#[derive(Clone)]
pub(crate) struct Match {
  pub path: PathBuf,
  pub line: usize,
  pub column: usize,
  pub preview: String,
  pub highlight: Range<usize>,
}

#[derive(Default)]
pub(crate) struct Results {
  pub matches: Vec<Match>,
  pub occurrences: usize,
  pub files: usize,
}

struct Matcher {
  pattern: Regex,
  include: GlobSet,
  exclude: GlobSet,
}

impl Matcher {
  fn new(options: &Options) -> Result<Self, String> {
    let source = if options.regex { options.query.clone() } else { regex::escape(&options.query) };
    let source = if options.whole_word { format!(r"\b(?:{source})\b") } else { source };
    let pattern = RegexBuilder::new(&source).case_insensitive(!options.case_sensitive).build().map_err(|error| error.to_string())?;
    Ok(Self { pattern, include: globs(&options.include)?, exclude: globs(&options.exclude)? })
  }

  fn files<'a>(&'a self, root: &'a Path) -> impl Iterator<Item = PathBuf> + 'a {
    ignore::WalkBuilder::new(root)
      .hidden(false)
      .git_ignore(true)
      .git_exclude(true)
      .require_git(false)
      .follow_links(false)
      .sort_by_file_path(Path::cmp)
      .filter_entry(|entry| entry.file_name() != ".git")
      .build()
      .filter_map(Result::ok)
      .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()) && entry.metadata().is_ok_and(|metadata| metadata.len() <= MAX_FILE_BYTES))
      .filter_map(move |entry| {
        let path = entry.into_path();
        let relative = path.strip_prefix(root).ok()?;
        ((self.include.is_empty() || self.include.is_match(relative)) && !self.exclude.is_match(relative)).then_some(path)
      })
  }
}

fn globs(value: &str) -> Result<GlobSet, String> {
  let mut builder = GlobSetBuilder::new();
  for pattern in value.split(',').map(str::trim).filter(|pattern| !pattern.is_empty()) {
    let pattern = pattern.trim_start_matches("./").trim_end_matches('/');
    let pattern = if pattern.contains('/') { pattern.to_owned() } else { format!("**/{pattern}") };
    for value in [pattern.clone(), format!("{pattern}/**")] {
      builder.add(GlobBuilder::new(&value).literal_separator(true).build().map_err(|error| error.to_string())?);
    }
  }
  builder.build().map_err(|error| error.to_string())
}

fn text_file(path: &Path) -> Option<String> {
  let text = fs::read_to_string(path).ok()?;
  (!text.contains('\0')).then_some(text)
}

pub(crate) fn search(root: &Path, options: &Options, cancelled: &AtomicBool) -> Result<Results, String> {
  if !root.is_dir() {
    return Err("The project folder is unavailable.".into());
  }
  if options.query.is_empty() {
    return Ok(Results::default());
  }
  let matcher = Matcher::new(options)?;
  let mut results = Results::default();
  for path in matcher.files(root) {
    let Some(text) = text_file(&path) else { continue };
    let before = results.occurrences;
    for (line, content) in text.lines().enumerate() {
      if cancelled.load(Ordering::Relaxed) {
        return Ok(results);
      }
      for found in matcher.pattern.find_iter(content) {
        results.occurrences += 1;
        if results.matches.len() == MAX_RESULTS {
          continue;
        }
        let start = found.start();
        let end = found.end();
        let context_start = content[..start].char_indices().rev().nth(12).map_or(0, |(offset, _)| offset);
        let context = content[context_start..start].trim_start();
        let prefix = if context_start > 0 { "…" } else { "" };
        let mut preview = format!("{prefix}{context}");
        let highlight_start = preview.len();
        preview.extend(content[start..end].chars().take(160));
        let highlight_end = preview.len();
        preview.extend(content[end..].chars().take(70));
        results.matches.push(Match {
          path: path.clone(),
          line,
          column: content[..start].encode_utf16().count(),
          preview,
          highlight: highlight_start..highlight_end,
        });
      }
    }
    if results.occurrences > before {
      results.files += 1;
    }
  }
  Ok(results)
}

pub(crate) struct Replacement {
  pub path: PathBuf,
  original: String,
  pub updated: String,
}

pub(crate) fn replacement_plan(root: &Path, options: &Options, replacement: &str, preserve_case: bool) -> Result<Vec<Replacement>, String> {
  if options.query.is_empty() {
    return Ok(Vec::new());
  }
  let matcher = Matcher::new(options)?;
  let mut plan = Vec::new();
  let mut bytes = 0;
  for path in matcher.files(root) {
    let Some(original) = text_file(&path) else { continue };
    let mut updated = String::with_capacity(original.len());
    for line in original.split_inclusive('\n') {
      let content = line.trim_end_matches('\n');
      let content = if line.ends_with("\r\n") { content.strip_suffix('\r').unwrap_or(content) } else { content };
      let replaced = matcher.pattern.replace_all(content, |captures: &Captures<'_>| {
        let mut value = String::new();
        if options.regex {
          captures.expand(replacement, &mut value);
        } else {
          value.push_str(replacement);
        }
        if preserve_case {
          let matched = &captures[0];
          if matched.chars().any(char::is_alphabetic) && !matched.chars().any(char::is_lowercase) {
            return value.to_uppercase();
          }
          if matched.chars().any(char::is_alphabetic) && !matched.chars().any(char::is_uppercase) {
            return value.to_lowercase();
          }
        }
        value
      });
      updated.push_str(&replaced);
      updated.push_str(&line[content.len()..]);
      if updated.len() > MAX_REPLACEMENT_BYTES {
        return Err("The replacement is too large. Limit the search to fewer files.".into());
      }
    }
    if updated != original {
      bytes += original.len() + updated.len();
      if bytes > MAX_REPLACEMENT_BYTES {
        return Err("The replacement is too large. Limit the search to fewer files.".into());
      }
      plan.push(Replacement { path, original, updated });
    }
  }
  Ok(plan)
}

pub(crate) fn apply_replacement(root: &Path, edit: &Replacement) -> std::io::Result<()> {
  let path = &edit.path;
  let fail = || std::io::Error::other("The file changed since the replacement was prepared. Search again.");
  let metadata = path.symlink_metadata()?;
  if !metadata.is_file() || !path.canonicalize()?.starts_with(root.canonicalize()?) || fs::read_to_string(path)? != edit.original {
    return Err(fail());
  }
  let parent = path.parent().ok_or_else(fail)?;
  let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
  temporary.as_file().set_permissions(metadata.permissions())?;
  temporary.write_all(edit.updated.as_bytes())?;
  if fs::read_to_string(path)? != edit.original {
    return Err(fail());
  }
  temporary.persist(path)?;
  Ok(())
}
