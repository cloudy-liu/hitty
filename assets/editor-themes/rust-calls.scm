; A called CamelCase constructor is a function; a bare variant is a value.
; Keep all-caps names under the bundled constant rule in both appearances.
((call_expression function: (identifier) @constructor)
 (#match? @constructor "^[A-Z]")
 (#not-match? @constructor "^[A-Z][A-Z\\d_]+$"))
((call_expression function: (scoped_identifier name: (identifier) @constructor))
 (#match? @constructor "^[A-Z]")
 (#not-match? @constructor "^[A-Z][A-Z\\d_]+$"))
((generic_function function: (identifier) @constructor)
 (#match? @constructor "^[A-Z]")
 (#not-match? @constructor "^[A-Z][A-Z\\d_]+$"))
((generic_function function: (scoped_identifier name: (identifier) @constructor))
 (#match? @constructor "^[A-Z]")
 (#not-match? @constructor "^[A-Z][A-Z\\d_]+$"))

; Qualified bare variants have an identifier outside the expression supertype.
((scoped_identifier name: (identifier) @constructor.variant)
 (#match? @constructor.variant "^[A-Z]")
 (#not-match? @constructor.variant "^[A-Z][A-Z\\d_]+$"))
