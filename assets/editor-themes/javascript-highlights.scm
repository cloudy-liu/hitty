; Distinguish authored const bindings from ordinary references.
(lexical_declaration
 kind: "const"
 (variable_declarator name: (identifier) @variable.constant
  value: [(number) (string) (true) (false) (null) (array) (object)
          (identifier) (binary_expression) (member_expression) (call_expression)
          (new_expression) (template_string) (unary_expression) (await_expression)]))
(lexical_declaration kind: "const"
 (variable_declarator name: (array_pattern (identifier) @variable.constant)))
; Relational/new operators use purple; expression operators use JS/TS cyan.
["in" "instanceof" "new"] @keyword.operator.word
["typeof" "void" "delete"] @operator
"default" @keyword.default
":" @punctuation.key_value
