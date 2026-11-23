#[allow(dead_code)]
#[path = "../src/minimap/outline.rs"]
mod outline;

use gpui_kit::{component::highlighter::HighlightTheme, HighlightStyle};
use std::ops::Range;
use ws_index::highlight::{Highlights, Language};

const ENV: &str = "# a comment\nTOKEN=abc/def\nexport MODE=production\nURL=https://www.example.com/path\n# OLD=https://old.example.com\n";

fn style_at(styles: &[(Range<usize>, HighlightStyle)], text: &str) -> HighlightStyle {
  let offset = ENV.find(text).unwrap();
  styles.iter().find(|(range, _)| range.contains(&offset)).map(|(_, style)| *style).unwrap()
}

#[test]
fn the_styles_cover_the_asked_range_in_both_themes() {
  let highlights = Highlights::compute(Language::Dotenv, ENV);
  for theme in [HighlightTheme::default_light(), HighlightTheme::default_dark()] {
    for range in [0..ENV.len(), 3..ENV.len() - 4, 0..1, ENV.find("URL").unwrap()..ENV.len()] {
      let styles = outline::line_styles(&highlights, &range, &*theme);
      let mut cursor = range.start;
      for (run, _) in &styles {
        assert_eq!(run.start, cursor);
        cursor = run.end;
      }
      assert_eq!(cursor, range.end);
    }
  }
}

#[test]
fn roles_become_the_themes_colors_and_underlines() {
  let highlights = Highlights::compute(Language::Dotenv, ENV);
  for theme in [HighlightTheme::default_light(), HighlightTheme::default_dark()] {
    let styles = outline::line_styles(&highlights, &(0..ENV.len()), &*theme);
    let comment = theme.style("comment").and_then(|style| style.color);
    let name = style_at(&styles, "TOKEN").color;
    assert!(name.is_some() && name != comment, "a name has a color of its own");
    assert_eq!(style_at(&styles, "# a comment").color, comment);
    assert_eq!(style_at(&styles, "abc/def").color, None, "a value keeps the text color");
    assert_eq!(style_at(&styles, "export").color, theme.style("keyword").and_then(|style| style.color));
    assert!(style_at(&styles, "https://www.example.com").underline.is_some());
    let in_comment = style_at(&styles, "https://old.example.com");
    assert!(in_comment.underline.is_some() && in_comment.color == comment, "an address in a comment keeps the comment color");
  }
}

#[test]
fn a_gitignore_comment_takes_the_comment_color_and_a_pattern_stays_plain() {
  let text = "# build output\n/build\n";
  let highlights = Highlights::compute(Language::Gitignore, text);
  let theme = HighlightTheme::default_dark();
  let styles = outline::line_styles(&highlights, &(0..text.len()), &*theme);
  assert_eq!(styles[0].1.color, theme.style("comment").and_then(|style| style.color));
  assert_eq!(styles.last().unwrap().1, HighlightStyle::default());
}

#[test]
fn dockerfile_instructions_use_the_name_color_and_variables_their_own() {
  let text = "FROM ${IMAGE} AS base\nRUN echo hi # not a comment\n";
  let highlights = Highlights::compute(Language::Dockerfile, text);
  for theme in [HighlightTheme::default_light(), HighlightTheme::default_dark()] {
    let styles = outline::line_styles(&highlights, &(0..text.len()), &*theme);
    let at = |needle: &str| {
      let offset = text.find(needle).unwrap();
      styles.iter().find(|(range, _)| range.contains(&offset)).map(|(_, style)| *style).unwrap()
    };
    let instruction = at("FROM").color;
    assert!(instruction.is_some(), "an instruction has a color");
    assert_eq!(at("AS base").color, instruction, "AS is drawn like the instructions");
    assert_eq!(at("RUN").color, instruction);
    assert_eq!(at("echo hi").color, None, "arguments keep the text color");
    assert_ne!(at("${IMAGE}"), at("echo hi"), "a variable is not plain text");
  }
}
