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

fn color_of(source: &str, language: &str, token: &str) -> Option<gpui_kit::Hsla> {
  syntax::register_highlights();
  let mut highlighter = SyntaxHighlighter::new(language);
  assert!(highlighter.update(None, &Rope::from(source), None));
  let theme = HighlightTheme::default_dark();
  let styles = highlighter.styles(&(0..source.len()), &*theme);
  let offset = source.find(token).unwrap();
  styles.iter().find(|(range, _)| range.contains(&offset)).and_then(|(_, style)| style.color)
}

fn role_color(role: &str) -> Option<gpui_kit::Hsla> {
  HighlightTheme::default_dark().style(role).and_then(|style| style.color)
}

#[test]
fn toml_tables_are_names() {
  let source = "[package]\nname = \"x\"\n";
  assert_eq!(color_of(source, "toml", "package"), role_color("function"));
  assert_eq!(color_of(source, "toml", "\"x\""), role_color("string"));
}

#[test]
fn yaml_and_json_keys_use_the_label_role() {
  assert_eq!(color_of("name: sample\n", "yaml", "name"), role_color("label"));
  assert_eq!(color_of("{\"name\": 1}", "json", "\"name\""), role_color("label"));
}

#[test]
fn shell_commands_are_constants_and_operators_keywords() {
  let source = "ls -la | cat\necho hi\n";
  assert_eq!(color_of(source, "bash", "echo"), role_color("constant"));
  assert_eq!(color_of(source, "bash", "|"), role_color("keyword"));
}
