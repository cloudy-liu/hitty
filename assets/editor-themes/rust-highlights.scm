; These broad variable fallbacks follow the bundled specific captures.
; Use grammar roles, so module paths and opaque macro token trees are excluded.
[
  (_expression/identifier)
  (_pattern/identifier)
  (shorthand_field_identifier)
] @variable
(captured_pattern (identifier) @variable)
(shorthand_field_initializer (identifier) @variable)
