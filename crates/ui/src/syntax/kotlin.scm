; Highlights for Kotlin (tree-sitter-kotlin-ng), written for Vekro: the grammar ships no query.

[(line_comment) (block_comment)] @comment
(shebang) @comment

[(string_literal) (multiline_string_literal) (character_literal)] @string
(escape_sequence) @string.escape

[(number_literal) (float_literal)] @number
; In this grammar `true`, `false` and `null` are plain identifiers.
((identifier) @boolean (#match? @boolean "^(true|false)$"))
((identifier) @constant (#eq? @constant "null"))
((identifier) @keyword (#match? @keyword "^(break|continue)$"))

(annotation) @attribute
(file_annotation) @attribute
(use_site_target) @attribute

(package_header (qualified_identifier) @namespace)
(import (qualified_identifier) @namespace)

(class_declaration name: (identifier) @type)
(object_declaration name: (identifier) @type)
(type_alias (identifier) @type)
(enum_entry (identifier) @constant)
(user_type (identifier) @type)
(type_parameter (identifier) @type)

(function_declaration name: (identifier) @function)
(call_expression (identifier) @function)
(call_expression (navigation_expression (identifier) @function .))
(constructor_invocation (user_type (identifier) @type))

(parameter (identifier) @variable.parameter)
(class_parameter (identifier) @variable.parameter)
(lambda_parameters (variable_declaration (identifier) @variable.parameter))
(property_declaration (variable_declaration (identifier) @variable))
(navigation_expression (identifier) @property .)
(this_expression) @variable.special
(super_expression) @variable.special
(label) @label

[
  (class_modifier) (member_modifier) (function_modifier) (property_modifier)
  (visibility_modifier) (inheritance_modifier) (parameter_modifier) (platform_modifier)
  (reification_modifier) (variance_modifier)
] @keyword

[
  "package" "import" "class" "interface" "object" "fun" "val" "var" "typealias"
  "constructor" "init" "companion" "get" "set" "by" "where" "enum"
  "if" "else" "when" "for" "while" "do" "try" "catch" "finally"
  "return" "return@" "throw" "in" "is" "as" "as?" "!in" "!is"
] @keyword

["+" "-" "*" "/" "%" "=" "+=" "-=" "*=" "/=" "%=" "==" "!=" "===" "!==" "<" ">" "<=" ">="
 "&&" "||" "!" "!!" "?:" "?." ".." "..<" "->" "::" "++" "--"] @operator
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," ":" ";" "."] @punctuation.delimiter
