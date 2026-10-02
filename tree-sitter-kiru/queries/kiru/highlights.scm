; Syntax highlighting for kiru.
;
; The palette is deliberately small. Tree-sitter's own guidance is that an
; entity should be colored the same wherever it appears, so a name never
; changes color between its declaration, its uses and its call sites, and a
; namespace is colored like any other name. Color is spent only on what a
; reader scans a file for: comments, strings, the keywords that shape a run,
; the two value kinds, and the names you can call. Every other name, whether a
; value, a parameter, a field, a record key or a namespace, shares one color.
;
; The captures are the standard Neovim names, so any colorscheme that supports
; Tree-sitter highlighting works without extra groups. Neovim applies patterns
; in order, so a general capture comes first and the more specific capture that
; shares a node comes later and wins.

; Comments and strings.
(comment) @comment
(string) @string

; The keywords that shape a run.
[
  "module"
  "import"
  "switch"
  "case"
  "default"
  "defer"
  "return"
  "panic"
] @keyword

; `fn` introduces a function; `txt` and `rec` are the two value kinds.
"fn" @keyword.function

[
  "txt"
  "rec"
] @type

; Every name is a value until a pattern below says it is callable, so a name
; keeps one color wherever it appears.
(identifier) @variable

; Kiru has one kind of callable. A function in the root namespace, a function
; in another namespace, a function the standard library provides and a method
; on a value are all entries in one registry, so all of them are named the
; same color as the function they are declared by.
(function_declaration
  name: (identifier) @function)

(call_expression
  callee: (path
    name: (identifier) @function))

; Operators.
[
  "+"
  "="
] @operator

; Brackets and delimiters.
[
  "("
  ")"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  ";"
  "."
  "::"
] @punctuation.delimiter
