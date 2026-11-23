use gpui_kit::component::highlighter::{LanguageConfig, LanguageRegistry};

pub(crate) fn register_highlights() {
  let registry = LanguageRegistry::singleton();
  if let Some(mut json) = registry.language("json") {
    json.highlights = r#"
      (comment) @comment
      (pair key: (string) @label)
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
  // The data and shell grammars, drawn as GitHub does: keys use the `label` role (green in the dark theme), a TOML table is a
  // function-like name, a shell command is a constant, and the shell's operators and flags are keywords.
  rewrite(registry, "yaml", |query| query.replace("@property", "@label"));
  rewrite(registry, "toml", |query| {
    format!(
      "{}\n(table (bare_key) @function)\n(table (dotted_key (bare_key) @function))\n(table_array_element (bare_key) @function)",
      query.replace("@property", "@variable").replace("\"=\" @operator", "\"=\" @punctuation.delimiter")
    )
  });
  rewrite(registry, "bash", |query| {
    // The last pattern colors every argument that starts with a dash, but the highlighter does not run its `#match?` test, so it
    // would color every word of a command. It goes.
    let query = query.rfind("(\n  (command").map_or(query, |at| &query[..at]);
    query
      .replace("(command_name) @function", "(command_name) @constant")
      .replace("(variable_name) @property", "(variable_name) @variable")
      .replace("] @operator", "] @keyword")
  });
  // Markdown as GitHub draws it: headings and rules in the heading style, quotes green, fences and inline marks like strings, link
  // brackets like strings, list markers orange and the pipes of a table gold (the `variant` role).
  rewrite(registry, "markdown", |query| {
    let query = query
      .replace("(setext_h2_underline)\n] @punctuation.special", "(setext_h2_underline)\n] @title")
      .replace("(block_continuation)\n  (block_quote_marker)\n] @punctuation.special", "(block_quote_marker)\n] @tag")
      .replace("(list_marker_parenthesis)\n  (thematic_break)\n] @punctuation.list_marker", "(list_marker_parenthesis)\n] @punctuation.list_marker")
      .replace("] @punctuation.delimiter", "] @string")
      .replace("@text.uri", "@link_uri");
    format!(
      "{query}\n(thematic_break) @title\n(block_quote (paragraph (inline) @tag))\n(pipe_table_delimiter_row) @variant\n(pipe_table_header \"|\" @variant)\n(pipe_table_row \"|\" @variant)"
    )
  });
  rewrite(registry, "markdown_inline", |query| {
    format!(
      "{}\n(inline_link [\"[\" \"]\" \"(\" \")\"] @string)\n(full_reference_link [\"[\" \"]\"] @string)\n(image [\"!\" \"[\" \"]\" \"(\" \")\"] @string)",
      query.replace("] @punctuation.delimiter", "] @string")
    )
  });
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

/// Replaces the highlight query of a registered language with the result of `change`.
fn rewrite(registry: &LanguageRegistry, name: &str, change: impl FnOnce(&str) -> String) {
  if let Some(mut language) = registry.language(name) {
    language.highlights = change(&language.highlights).into();
    registry.register(name, &language);
  }
}
