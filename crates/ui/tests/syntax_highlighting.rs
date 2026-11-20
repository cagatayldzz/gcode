#[path = "../src/syntax.rs"]
mod syntax;

use gpui_kit::{
  base::input::{HighlightStyleResolver as _, Rope},
  component::highlighter::{HighlightTheme, SyntaxHighlighter},
};

#[test]
fn swift_uses_semantic_colors_in_both_themes() {
  syntax::register_highlights();
  let source = "// A greeting.\nstruct User {\n  let name: String\n  func greet() -> String { \"Hello \\(name)\" }\n}\n";
  let mut highlighter = SyntaxHighlighter::new("swift");
  assert!(highlighter.update(None, &Rope::from(source), None));

  for theme in [HighlightTheme::default_light(), HighlightTheme::default_dark()] {
    let styles = highlighter.styles(&(0..source.len()), &*theme);
    for (token, role) in [("// A greeting.", "comment"), ("struct", "keyword"), ("greet()", "function"), ("String", "type"), ("Hello ", "string")] {
      let offset = source.find(token).unwrap();
      let expected = theme.style(role).and_then(|style| style.color).unwrap();
      let actual = styles.iter().find(|(range, _)| range.contains(&offset)).and_then(|(_, style)| style.color);
      assert_eq!(actual, Some(expected), "{token} should use the {role} color");
    }
  }
}
