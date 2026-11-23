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

#[test]
fn kotlin_uses_semantic_colors_in_both_themes() {
  syntax::register_highlights();
  let source =
    "// Says hi.\n@Composable\nfun greet(name: String?): Boolean {\n  if (name == null) return false\n  return true\n}\nclass User(val id: Int)\n";
  let mut highlighter = SyntaxHighlighter::new("kotlin");
  assert!(highlighter.update(None, &Rope::from(source), None));

  for theme in [HighlightTheme::default_light(), HighlightTheme::default_dark()] {
    let styles = highlighter.styles(&(0..source.len()), &*theme);
    for (token, role) in
      [("// Says hi.", "comment"), ("fun", "keyword"), ("greet", "function"), ("String", "type"), ("class", "keyword"), ("User", "type")]
    {
      let offset = source.find(token).unwrap();
      let expected = theme.style(role).and_then(|style| style.color).unwrap();
      let actual = styles.iter().find(|(range, _)| range.contains(&offset)).and_then(|(_, style)| style.color);
      assert_eq!(actual, Some(expected), "{token} should use the {role} color");
    }
  }
}

#[test]
fn tsx_colors_jsx_tags_attributes_and_typescript() {
  syntax::register_highlights();
  let source = "import React from \"react\";\ninterface Props { label: string }\nexport function Button({ label }: Props) {\n  const n = 1;\n  return <div className=\"a\"><Icon size={n} />{label}</div>;\n}\n";
  let mut highlighter = SyntaxHighlighter::new("tsx");
  assert!(highlighter.update(None, &Rope::from(source), None));

  for theme in [HighlightTheme::default_light(), HighlightTheme::default_dark()] {
    let styles = highlighter.styles(&(0..source.len()), &*theme);
    for (token, role) in [
      ("import", "keyword"),
      ("\"react\"", "string"),
      ("interface", "keyword"),
      ("string", "type"),
      ("function", "keyword"),
      ("div className", "tag"),
      ("className", "attribute"),
      ("Icon", "type"),
      ("1;", "number"),
    ] {
      let offset = source.find(token).unwrap();
      let expected = theme.style(role).and_then(|style| style.color).unwrap_or_else(|| panic!("the theme has no {role} color"));
      let actual = styles.iter().find(|(range, _)| range.contains(&offset)).and_then(|(_, style)| style.color);
      assert_eq!(actual, Some(expected), "{token} should use the {role} color");
    }
  }
}
