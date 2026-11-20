#[allow(dead_code)]
#[path = "../src/minimap/outline.rs"]
mod outline;

use gpui_kit::{
  base::input::{HighlightStyleResolver as _, Rope},
  component::highlighter::HighlightTheme,
  HighlightStyle,
};
use std::ops::Range;

const SOURCE: &str = "# a comment\nNEXT_PUBLIC_TOKEN=abc/def+ghi==\nexport MODE=production\n\n  ## Endpoint: https://test.example.cloud\nURL=https://www.example.com/path\n# OLD_KEY=https://old.example.com\n";

fn styles(theme: &HighlightTheme) -> Vec<(Range<usize>, HighlightStyle)> {
  let text = Rope::from(SOURCE);
  outline::dotenv_styles(&text, &(0..SOURCE.len()), theme)
}

fn style_at(styles: &[(Range<usize>, HighlightStyle)], text: &str) -> Option<HighlightStyle> {
  let offset = SOURCE.find(text).unwrap();
  styles.iter().find(|(range, _)| range.contains(&offset)).map(|(_, style)| *style)
}

fn text_of(styles: &[(Range<usize>, HighlightStyle)], offset: usize) -> &'static str {
  let (range, _) = styles.iter().find(|(range, _)| range.contains(&offset)).unwrap();
  &SOURCE[range.clone()]
}

#[test]
fn names_comments_and_addresses_are_colored_in_both_themes() {
  for theme in [HighlightTheme::default_light(), HighlightTheme::default_dark()] {
    let styles = styles(&theme);
    let comment = theme.style("comment").and_then(|style| style.color);
    let name = style_at(&styles, "NEXT_PUBLIC_TOKEN").and_then(|style| style.color);
    assert!(name.is_some(), "a name has a color");
    assert_ne!(name, comment, "a name is not drawn like a comment");
    assert_eq!(style_at(&styles, "# a comment").and_then(|style| style.color), comment, "a # line is a comment");
    assert_eq!(style_at(&styles, "# OLD_KEY").and_then(|style| style.color), comment, "a commented-out setting is a comment");
    assert_eq!(style_at(&styles, "MODE").and_then(|style| style.color), name, "a name after export has the same color");
    assert_eq!(style_at(&styles, "export").and_then(|style| style.color), theme.style("keyword").and_then(|style| style.color));
  }
}

#[test]
fn the_name_stops_at_the_equals_sign_and_the_value_keeps_the_text_color() {
  let styles = styles(&HighlightTheme::default_dark());
  let offset = SOURCE.find("NEXT_PUBLIC_TOKEN").unwrap();
  assert_eq!(text_of(&styles, offset), "NEXT_PUBLIC_TOKEN");
  assert!(style_at(&styles, "abc/def+ghi==").is_none(), "a value is left in the text color");
}

#[test]
fn web_addresses_are_underlined_up_to_the_next_space_even_in_comments() {
  let styles = styles(&HighlightTheme::default_dark());
  for address in ["https://test.example.cloud", "https://www.example.com/path", "https://old.example.com"] {
    let offset = SOURCE.find(address).unwrap();
    assert_eq!(text_of(&styles, offset), address);
    assert!(styles.iter().any(|(range, style)| range.contains(&offset) && style.underline.is_some()), "{address} is underlined");
  }
  assert!(style_at(&styles, "Endpoint:").is_none_or(|style| style.underline.is_none()), "only the address is underlined");
}

#[test]
fn only_the_asked_range_is_styled() {
  let text = Rope::from(SOURCE);
  let start = SOURCE.find("URL=").unwrap();
  let range = start..start + 3;
  let styles = outline::dotenv_styles(&text, &range, &*HighlightTheme::default_dark());
  assert!(styles.iter().all(|(span, _)| span.start >= range.start && span.end <= range.end));
  assert_eq!(styles.len(), 1);
}
