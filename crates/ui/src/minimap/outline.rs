//! Bounded samples of buffer lines for the minimap.

use gpui_kit::{
  base::input::{HighlightStyleResolver, Rope, RopeExt as _},
  component::highlighter::{HighlightTheme, SyntaxHighlighter},
  px, HighlightStyle, Hsla, SharedString, UnderlineStyle,
};
use std::{ops::Range, sync::Arc};
use ws_index::highlight::{Highlights, Language, Role};

const MAX_SAMPLES: usize = 640;
const MAX_COLUMNS: usize = 100;

pub(super) struct Line {
  pub text: SharedString,
  pub colors: Vec<(usize, Option<Hsla>)>,
}

pub(super) fn sample(text: &Rope, language: &str, theme: &HighlightTheme) -> Arc<[Line]> {
  // The line-based languages are classified once for the whole text; the others go through the syntax highlighter.
  let line_language = Language::from_name(language);
  let line_highlights = line_language.map(|language| Highlights::compute(language, &text.to_string()));
  let highlighter = line_language.is_none().then(|| {
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
        let styles = match (&highlighter, &line_highlights) {
          (Some(highlighter), _) => highlighter.styles(&range, theme),
          (None, Some(highlights)) => line_styles(highlights, &range, theme),
          (None, None) => Vec::new(),
        };
        styles.into_iter().map(|(range, style)| (range.len(), style.color)).collect()
      };
      Line { text: SharedString::from(line.to_owned()), colors }
    })
    .collect()
}

/// The runs of `highlights` inside `range` with the theme's style for each role. They cover `range` completely, as the editor
/// requires. Nothing is read from the text: the classification was done once, before the file was shown.
pub(crate) fn line_styles(
  highlights: &Highlights,
  range: &Range<usize>,
  resolver: &dyn HighlightStyleResolver,
) -> Vec<(Range<usize>, HighlightStyle)> {
  let comment = resolver.style("comment").unwrap_or_default();
  let underline = Some(UnderlineStyle { thickness: px(1.), color: None, wavy: false });
  let name = orange_style(resolver);
  let keyword = resolver.style("keyword").unwrap_or_default();
  let variable = resolver.style("variable").or_else(|| resolver.style("property")).unwrap_or_default();
  highlights
    .slice(range)
    .into_iter()
    .map(|(run, role)| {
      let style = match role {
        None => HighlightStyle::default(),
        Some(Role::Comment) => comment,
        Some(Role::Name | Role::Instruction) => name,
        Some(Role::Keyword) => keyword,
        Some(Role::Variable) => variable,
        Some(Role::Link) => HighlightStyle { underline, ..HighlightStyle::default() },
        Some(Role::CommentLink) => HighlightStyle { underline, ..comment },
      };
      (run, style)
    })
    .collect()
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
