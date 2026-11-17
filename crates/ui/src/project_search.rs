//! Search project files without blocking the interface.

use std::{
  fs,
  ops::Range,
  path::{Path, PathBuf},
  sync::atomic::{AtomicBool, Ordering},
};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub(crate) const MAX_RESULTS: usize = 200;

#[derive(Clone)]
pub(crate) struct Match {
  pub path: PathBuf,
  pub line: usize,
  pub column: usize,
  pub preview: String,
  pub highlight: Range<usize>,
}

pub(crate) fn search(root: &Path, query: &str, cancelled: &AtomicBool) -> Result<Vec<Match>, String> {
  if !root.is_dir() {
    return Err("The project folder is unavailable.".into());
  }
  let needle = query.to_lowercase();
  let mut matches = Vec::new();
  for entry in ignore::WalkBuilder::new(root)
    .hidden(false)
    .git_ignore(true)
    .git_exclude(true)
    .require_git(false)
    .follow_links(false)
    .filter_entry(|entry| entry.file_name() != ".git")
    .build()
    .flatten()
  {
    if cancelled.load(Ordering::Relaxed) {
      break;
    }
    if !entry.file_type().is_some_and(|kind| kind.is_file()) || entry.metadata().map_or(true, |metadata| metadata.len() > MAX_FILE_BYTES) {
      continue;
    }
    let Ok(bytes) = fs::read(entry.path()) else { continue };
    if bytes.contains(&0) {
      continue;
    }
    let Ok(text) = std::str::from_utf8(&bytes) else { continue };
    for (line, content) in text.lines().enumerate() {
      if cancelled.load(Ordering::Relaxed) {
        return Ok(matches);
      }
      let lower = content.to_lowercase();
      if let Some(column) = lower.find(&needle) {
        let mut folded_bytes = 0;
        let mut utf16_column = 0;
        let mut start = 0;
        let mut end = content.len();
        for (offset, ch) in content.char_indices() {
          if folded_bytes >= column + needle.len() {
            end = offset;
            break;
          }
          let next = folded_bytes + ch.to_lowercase().map(char::len_utf8).sum::<usize>();
          if next <= column {
            utf16_column += ch.len_utf16();
          } else if folded_bytes <= column {
            start = offset;
          }
          folded_bytes = next;
        }
        let context_start = content[..start].char_indices().rev().nth(12).map_or(0, |(offset, _)| offset);
        let context = content[context_start..start].trim_start();
        let prefix = if context_start > 0 { "…" } else { "" };
        let mut preview = format!("{prefix}{context}");
        let highlight_start = preview.len();
        preview.extend(content[start..end].chars().take(160));
        let highlight_end = preview.len();
        preview.extend(content[end..].chars().take(40));
        matches.push(Match { path: entry.path().to_path_buf(), line, column: utf16_column, preview, highlight: highlight_start..highlight_end });
        if matches.len() == MAX_RESULTS {
          return Ok(matches);
        }
      }
    }
  }
  Ok(matches)
}
