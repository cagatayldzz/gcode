//! Fuzzy matching of a short query against file paths, for opening a file by a few of its letters.
//!
//! The letters of the query must appear in the path in order, case ignored. A match scores higher when its letters follow each
//! other, start a word (after `/`, `_`, `-`, `.` or a space, or a capital after a small letter) or fall in the file name; a
//! shorter path wins a tie.

use std::cmp::Reverse;

const NONE: i32 = i32::MIN / 2;
const MATCH: i32 = 1;
const CONSECUTIVE: i32 = 6;
const WORD_START: i32 = 8;
const IN_FILE_NAME: i32 = 3;

fn is_separator(character: char) -> bool {
  matches!(character, '/' | '\\' | '_' | '-' | '.' | ' ')
}

/// How well `query` matches `candidate`, or `None` when its letters do not appear in order. A higher score is a better match.
pub fn score(query: &str, candidate: &str) -> Option<i32> {
  let query: Vec<char> = query.chars().filter(|character| !character.is_whitespace()).flat_map(char::to_lowercase).collect();
  if query.is_empty() {
    return Some(0);
  }
  let original: Vec<char> = candidate.chars().collect();
  if query.len() > original.len() {
    return None;
  }
  let lower: Vec<char> = original.iter().map(|character| character.to_lowercase().next().unwrap_or(*character)).collect();
  let file_name_start = original.iter().rposition(|character| *character == '/').map_or(0, |index| index + 1);

  // The score of each position as the place where the match of one more letter of the query ends.
  let gain = |index: usize| {
    let mut gain = MATCH;
    if index == 0 || is_separator(original[index - 1]) || (original[index - 1].is_lowercase() && original[index].is_uppercase()) {
      gain += WORD_START;
    }
    if index >= file_name_start {
      gain += IN_FILE_NAME;
    }
    gain
  };
  let mut previous = vec![NONE; original.len()];
  for (index, slot) in previous.iter_mut().enumerate() {
    if lower[index] == query[0] {
      // Letters skipped before the first match cost a little, up to a limit.
      *slot = gain(index) - (index as i32).min(3);
    }
  }
  for letter in &query[1..] {
    let mut current = vec![NONE; original.len()];
    // The best earlier ending, with the skipped letters between it and here already taken off.
    let mut best_gap = NONE;
    for index in 1..original.len() {
      if index >= 2 && previous[index - 2] > NONE {
        best_gap = best_gap.max(previous[index - 2] + (index as i32 - 2));
      }
      if lower[index] != *letter {
        continue;
      }
      let mut best = NONE;
      if previous[index - 1] > NONE {
        best = previous[index - 1] + CONSECUTIVE;
      }
      if best_gap > NONE {
        best = best.max(best_gap - (index as i32 - 1));
      }
      if best > NONE {
        current[index] = best + gain(index);
      }
    }
    previous = current;
  }
  let best = previous.into_iter().max().filter(|best| *best > NONE)?;
  Some(best - (original.len() as i32) / 8)
}

/// The indexes of the best `limit` candidates for `query`, best first. An empty query keeps the first `limit` in their order.
pub fn rank<'a>(query: &str, candidates: impl IntoIterator<Item = &'a str>, limit: usize) -> Vec<usize> {
  let mut found: Vec<(i32, usize)> =
    candidates.into_iter().enumerate().filter_map(|(index, candidate)| score(query, candidate).map(|score| (score, index))).collect();
  if query.trim().is_empty() {
    found.truncate(limit);
  } else {
    found.sort_by_key(|(score, index)| (Reverse(*score), *index));
    found.truncate(limit);
  }
  found.into_iter().map(|(_, index)| index).collect()
}
