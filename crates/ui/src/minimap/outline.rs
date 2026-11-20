//! Bounded samples of buffer lines for the minimap.

use gpui_kit::{
  base::input::{HighlightStyleResolver, Rope, RopeExt as _},
  component::highlighter::{HighlightTheme, SyntaxHighlighter},
  px, HighlightStyle, Hsla, SharedString, UnderlineStyle,
};
use std::{ops::Range, sync::Arc};

const MAX_SAMPLES: usize = 640;
const MAX_COLUMNS: usize = 100;

pub(super) struct Line {
  pub text: SharedString,
  pub colors: Vec<(usize, Option<Hsla>)>,
}

pub(super) fn sample(text: &Rope, language: &str, theme: &HighlightTheme) -> Arc<[Line]> {
  let highlighter = (!matches!(language, "gitignore" | "dotenv")).then(|| {
    let mut highlighter = SyntaxHighlighter::new(language);
    highlighter.update(None, text, None);
    highlighter
  });
  let count = text.lines_len();
  let samples = count.min(MAX_SAMPLES);
  (0..samples)
    .map(|index| {
      let row = index * count / samples;
      let mut line = String::new();
      for character in text.slice_line(row).chars().take(MAX_COLUMNS) {
        match character {
          '\r' | '\n' => break,
          '\t' => line.push(' '),
          _ => line.push(character),
        }
      }
      let line = line.trim_end();
      let start = text.line_start_offset(row);
      let colors = if line.is_empty() {
        Vec::new()
      } else {
        let range = start..start + line.len();
        let styles = match &highlighter {
          Some(highlighter) => highlighter.styles(&range, theme),
          None if language == "dotenv" => dotenv_styles(text, &range, theme),
          None => gitignore_styles(text, &range, theme),
        };
        styles.into_iter().map(|(range, style)| (range.len(), style.color)).collect()
      };
      Line { text: SharedString::from(line.to_owned()), colors }
    })
    .collect()
}

/// Git treats a hash as a comment only at the beginning of a line.
pub(crate) fn gitignore_styles(text: &Rope, range: &Range<usize>, resolver: &dyn HighlightStyleResolver) -> Vec<(Range<usize>, HighlightStyle)> {
  let comment = resolver.style("comment").unwrap_or_default();
  let start_row = text.offset_to_point(range.start).row;
  let end_row = text.offset_to_point(range.end).row;
  (start_row..=end_row)
    .filter_map(|row| {
      let line_start = text.line_start_offset(row);
      let start = range.start.max(line_start);
      let end = range.end.min(text.line_end_offset(row));
      let style = if text.char_at(line_start) == Some('#') { comment } else { HighlightStyle::default() };
      (start < end).then_some((start..end, style))
    })
    .collect()
}

/// How `.env` files are colored: a `#` line is a comment, the name before the first `=` takes an accent color (the theme's
/// orange-looking role), `export` is a keyword, and every web address is underlined.
pub(crate) fn dotenv_styles(text: &Rope, range: &Range<usize>, resolver: &dyn HighlightStyleResolver) -> Vec<(Range<usize>, HighlightStyle)> {
  let comment = resolver.style("comment").unwrap_or_default();
  let keyword = resolver.style("keyword").unwrap_or_default();
  let name = orange_style(resolver);
  let link = HighlightStyle { underline: Some(UnderlineStyle { thickness: px(1.), color: None, wavy: false }), ..HighlightStyle::default() };
  let start_row = text.offset_to_point(range.start).row;
  let end_row = text.offset_to_point(range.end).row;
  let mut styles = Vec::new();
  for row in start_row..=end_row {
    let line_start = text.line_start_offset(row);
    let line: String = text.slice_line(row).chars().take_while(|character| !matches!(character, '\n' | '\r')).collect();
    let indent = line.len() - line.trim_start().len();
    let body = &line[indent..];
    // Web addresses are underlined wherever they are, comments included.
    let mut addresses: Vec<Range<usize>> = Vec::new();
    let mut from = 0;
    while let Some(found) = line[from..].find("http://").into_iter().chain(line[from..].find("https://")).min() {
      let begin = from + found;
      let end = line[begin..].find(char::is_whitespace).map_or(line.len(), |length| begin + length);
      addresses.push(begin..end);
      from = end;
    }
    let mut spans: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    if body.starts_with('#') {
      // The comment color around the addresses, which keep it under their underline.
      let mut cursor = indent;
      for address in &addresses {
        spans.push((cursor..address.start, comment));
        spans.push((address.clone(), HighlightStyle { underline: link.underline, ..comment }));
        cursor = address.end;
      }
      spans.push((cursor..line.len(), comment));
    } else {
      let (export, rest) = match body.strip_prefix("export") {
        Some(rest) if rest.starts_with([' ', '\t']) => ("export".len(), &rest[rest.len() - rest.trim_start().len()..]),
        _ => (0, body),
      };
      let key_start = line.len() - rest.len();
      if export > 0 {
        spans.push((indent..indent + export, keyword));
      }
      if let Some(equals) = rest.find('=') {
        let key = &rest[..equals];
        if !key.is_empty() && key.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')) {
          spans.push((key_start..key_start + equals, name));
        }
      }
      spans.extend(addresses.into_iter().map(|address| (address, link)));
    }
    spans.retain(|(span, _)| span.start < span.end);
    spans.sort_by_key(|(span, _)| span.start);
    for (span, style) in spans {
      let (start, end) = ((line_start + span.start).max(range.start), (line_start + span.end).min(range.end));
      if start < end {
        styles.push((start..end, style));
      }
    }
  }
  styles
}

/// The style of the theme's role that looks the most orange, so that names stand out in any theme (gold in the dark one, brown
/// in the light one).
fn orange_style(resolver: &dyn HighlightStyleResolver) -> HighlightStyle {
  const ORANGE: f32 = 38.;
  ["constant", "attribute", "number", "type", "string"]
    .into_iter()
    .filter_map(|role| resolver.style(role))
    .filter_map(|style| style.color.map(|color| (style, (color.h * 360. - ORANGE).abs())))
    .min_by(|(_, left), (_, right)| left.total_cmp(right))
    .map(|(style, _)| style)
    .unwrap_or_default()
}
