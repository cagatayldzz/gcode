(jsx_opening_element name: (identifier) @tag (#match? @tag "^[a-z][^.]*$"))
(jsx_closing_element name: (identifier) @tag (#match? @tag "^[a-z][^.]*$"))
(jsx_self_closing_element name: (identifier) @tag (#match? @tag "^[a-z][^.]*$"))

(jsx_opening_element name: (identifier) @type (#match? @type "^[A-Z]"))
(jsx_closing_element name: (identifier) @type (#match? @type "^[A-Z]"))
(jsx_self_closing_element name: (identifier) @type (#match? @type "^[A-Z]"))

(jsx_opening_element name: (member_expression object: (identifier) @type property: (property_identifier) @type))
(jsx_closing_element name: (member_expression object: (identifier) @type property: (property_identifier) @type))
(jsx_self_closing_element name: (member_expression object: (identifier) @type property: (property_identifier) @type))

(jsx_attribute (property_identifier) @attribute)
(jsx_attribute (jsx_namespace_name) @attribute)

(jsx_opening_element ["<" ">"] @punctuation.bracket)
(jsx_closing_element ["</" ">"] @punctuation.bracket)
(jsx_self_closing_element ["<" "/>"] @punctuation.bracket)
(jsx_expression ["{" "}"] @punctuation.bracket)

(jsx_text) @string.special
