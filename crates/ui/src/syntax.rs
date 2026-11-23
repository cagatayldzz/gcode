use gpui_kit::component::highlighter::{LanguageConfig, LanguageRegistry};

pub(crate) fn register_highlights() {
  let registry = LanguageRegistry::singleton();
  if let Some(mut json) = registry.language("json") {
    json.highlights = r#"
      (comment) @comment
      (pair key: (string) @property)
      (pair value: (string) @string)
      (array (string) @string)
      (document (string) @string)
      (escape_sequence) @string.escape
      (number) @number
      [(true) (false)] @boolean
      (null) @constant
      ["{" "}" "[" "]"] @punctuation.bracket
      ["," ":"] @punctuation.delimiter
    "#
    .into();
    registry.register("json", &json);
  }
  // GPUI Kit registers the Swift grammar with an empty highlight query.
  if let Some(mut swift) = registry.language("swift") {
    swift.highlights = tree_sitter_swift::HIGHLIGHTS_QUERY.into();
    registry.register("swift", &swift);
  }
  // GPUI Kit has no Kotlin grammar: the grammar crate comes with the app, and so does the highlight query.
  registry.register(
    "kotlin",
    &LanguageConfig::new("kotlin", tree_sitter_kotlin_ng::LANGUAGE.into(), Vec::new(), include_str!("syntax/kotlin.scm"), "", ""),
  );
  // GPUI Kit gives TSX the bare grammar query and JavaScript no JSX rules, so no JSX tag is colored. Add them.
  if let (Some(mut tsx), Some(typescript)) = (registry.language("tsx"), registry.language("typescript")) {
    tsx.highlights = format!("{}\n{}", typescript.highlights, include_str!("syntax/jsx.scm")).into();
    registry.register("tsx", &tsx);
  }
  if let Some(mut javascript) = registry.language("javascript") {
    javascript.highlights = format!("{}\n{}", javascript.highlights, include_str!("syntax/jsx.scm")).into();
    registry.register("javascript", &javascript);
  }
}
