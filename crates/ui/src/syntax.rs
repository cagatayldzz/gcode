use gpui_kit::component::highlighter::LanguageRegistry;

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
}
