; Local scope and binding queries for kiru.
;
; Names are read top-down and a local shadows an outer binding. A function is
; a scope, so its parameters are visible throughout its body. A block is a
; scope for the `txt` and `rec` bindings it declares. Each `case` arm is its
; own scope that still sees the enclosing body. A `defer` body is its own
; scope.

; Scopes.
(function_declaration) @scope
(block) @scope
(case_clause) @scope
(defer_statement) @scope

; Definitions.
(parameter
  name: (identifier) @definition.parameter)

(text_binding
  name: (identifier) @definition.var)

(record_binding
  name: (identifier) @definition.var)

(assignment_statement
  name: (identifier) @definition.var)

; References. Path segments resolve against the definitions above.
(identifier) @reference
