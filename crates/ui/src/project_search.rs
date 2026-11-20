//! Search and replace project text on the background executor.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::{Captures, Regex, RegexBuilder};
use std::{
  fs,
  io::{Read as _, Write},
  ops::Range,
  path::{Path, PathBuf},
  sync::atomic::{AtomicBool, AtomicUsize, Ordering},
  thread,
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

  fn files(&self, root: &Path) -> Vec<PathBuf> {
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
      .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
      .filter_map(|entry| {
        let path = entry.into_path();
        let relative = path.strip_prefix(root).ok()?;
        ((self.include.is_empty() || self.include.is_match(relative)) && !self.exclude.is_match(relative)).then_some(path)
      })
      .collect()
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
  let mut file = fs::File::open(path).ok()?;
  let length = file.metadata().ok()?.len();
  if length > MAX_FILE_BYTES {
    return None;
  }
  let mut text = String::with_capacity(length as usize);
  file.read_to_string(&mut text).ok()?;
  (!text.contains('\0')).then_some(text)
}

/// Runs `work` on every path across worker threads and returns the results in path order.
fn map_files<T: Send>(paths: &[PathBuf], work: impl Fn(&Path) -> T + Sync) -> Vec<T> {
  let workers = thread::available_parallelism().map_or(1, usize::from).min(paths.len());
  if workers <= 1 {
    return paths.iter().map(|path| work(path)).collect();
  }
  let next = AtomicUsize::new(0);
  let mut done: Vec<(usize, T)> = thread::scope(|scope| {
    let handles: Vec<_> = (0..workers)
      .map(|_| {
        scope.spawn(|| {
          let mut local = Vec::new();
          loop {
            let index = next.fetch_add(1, Ordering::Relaxed);
            let Some(path) = paths.get(index) else { break local };
            local.push((index, work(path)));
          }
        })
      })
      .collect();
    handles.into_iter().flat_map(|handle| handle.join().expect("search worker panicked")).collect()
  });
  done.sort_unstable_by_key(|(index, _)| *index);
  done.into_iter().map(|(_, value)| value).collect()
}

fn preview_match(path: &Path, line: usize, content: &str, start: usize, end: usize) -> Match {
  let context_start = content[..start].char_indices().rev().nth(12).map_or(0, |(offset, _)| offset);
  let context = content[context_start..start].trim_start();
  let prefix = if context_start > 0 { "…" } else { "" };
  let mut preview = String::with_capacity(prefix.len() + context.len() + 240);
  preview.push_str(prefix);
  preview.push_str(context);
  let highlight_start = preview.len();
  preview.extend(content[start..end].chars().take(160));
  let highlight_end = preview.len();
  preview.extend(content[end..].chars().take(70));
  Match { path: path.to_owned(), line, column: content[..start].encode_utf16().count(), preview, highlight: highlight_start..highlight_end }
}

pub(crate) fn search(root: &Path, options: &Options, cancelled: &AtomicBool) -> Result<Results, String> {
  if !root.is_dir() {
    return Err("The project folder is unavailable.".into());
  }
  if options.query.is_empty() {
    return Ok(Results::default());
  }
  let matcher = Matcher::new(options)?;
  let paths: Vec<PathBuf> = matcher.files(root);
  let found = map_files(&paths, |path| {
    let mut file = (Vec::new(), 0);
    if cancelled.load(Ordering::Relaxed) {
      return file;
    }
    let Some(text) = text_file(path) else { return file };
    for (line, content) in text.lines().enumerate() {
      for found in matcher.pattern.find_iter(content) {
        file.1 += 1;
        if file.0.len() < MAX_RESULTS {
          file.0.push(preview_match(path, line, content, found.start(), found.end()));
        }
      }
    }
    file
  });
  let mut results = Results::default();
  for (mut matches, occurrences) in found {
    results.occurrences += occurrences;
    results.files += usize::from(occurrences > 0);
    matches.truncate(MAX_RESULTS - results.matches.len());
    results.matches.append(&mut matches);
  }
  Ok(results)
}

pub(crate) struct Replacement {
  pub path: PathBuf,
  original: String,
  pub updated: String,
}

const TOO_LARGE: &str = "The replacement is too large. Limit the search to fewer files.";

fn replace_file(path: &Path, matcher: &Matcher, options: &Options, replacement: &str, preserve_case: bool) -> Result<Option<Replacement>, String> {
  let Some(original) = text_file(path) else { return Ok(None) };
  if !matcher.pattern.is_match(&original) {
    return Ok(None);
  }
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
      return Err(TOO_LARGE.into());
    }
  }
  Ok((updated != original).then(|| Replacement { path: path.to_owned(), original, updated }))
}

pub(crate) fn replacement_plan(root: &Path, options: &Options, replacement: &str, preserve_case: bool) -> Result<Vec<Replacement>, String> {
  if options.query.is_empty() {
    return Ok(Vec::new());
  }
  let matcher = Matcher::new(options)?;
  let paths: Vec<PathBuf> = matcher.files(root);
  let mut plan = Vec::new();
  let mut bytes = 0;
  for edit in map_files(&paths, |path| replace_file(path, &matcher, options, replacement, preserve_case)) {
    let Some(edit) = edit? else { continue };
    bytes += edit.original.len() + edit.updated.len();
    if bytes > MAX_REPLACEMENT_BYTES {
      return Err(TOO_LARGE.into());
    }
    plan.push(edit);
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
