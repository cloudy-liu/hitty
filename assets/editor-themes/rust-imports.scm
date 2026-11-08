; Match the upstream Rust grammar's type classification in use paths while
; leaving conventional all-caps constants to the original constant rules.
((use_declaration argument: (scoped_identifier name: (identifier) @type))
 (#match? @type "^[A-Z]")
 (#not-match? @type "^[A-Z][A-Z\\d_]+$"))
