; Python identifiers are plain text; magic names retain the authored red.
((identifier) @variable.builtin
 (#match? @variable.builtin "^__[A-Za-z0-9_]+__$"))
(parameters (identifier) @variable.parameter)
(default_parameter name: (identifier) @variable.parameter)
(typed_parameter (identifier) @variable.parameter)
(typed_default_parameter name: (identifier) @variable.parameter)
; Imports must not inherit the grammar's capitalized-name constructor guess.
(dotted_name (identifier) @variable)
(class_definition name: (identifier) @type)
(typed_parameter (type (identifier) @type.parameter))
(typed_default_parameter type: (type (identifier) @type.parameter))
