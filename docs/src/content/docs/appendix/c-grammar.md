---
title: Appendix C. Grammar
description: The accepted syntax in one place.
---

A compact grammar of the language. Terminals are quoted; `*` means zero or
more; `?` means optional.

```text
file        := module? import* declaration*
module      := "module" path ";"
import      := "import" string ";"

declaration := function | text | record
function    := "fn" ident "(" params? ")" ("->" kind)? block ";"
text        := "mut"? "txt" ident "=" expression ";"
record      := "mut"? "rec" ident "=" expression ";"

params      := param ("," param)* ","?
param       := "mut"? kind ident
kind        := "txt" | "rec"
fields      := "{" (field ("," field)* ","?)? "}"
field       := ident "=" expression
path        := "::"? ident ("::" ident)*

block       := "{" statement* "}"
statement   := text
             | record
             | assignment
             | field_assignment
             | expression ";"
             | "return" "(" expression? ")" ";"
             | "panic" ";"
             | "async" expression ";"
             | "wait" ";"
             | "switch" "(" expression ")" switch ";"
             | "defer" block ";"
assignment  := ident "=" expression ";"
field_assignment := ident "." ident "=" expression ";"

switch      := "{" (case | default)* "}"
case        := "case" "(" expression ")" block ";"
default     := "default" block ";"

expression  := term ("+" term)*
term        := primary ("." ident)*
primary     := string
             | fields
             | path arguments?
             | path
arguments   := "(" (expression ("," expression)* ","?)? ")"
```

## Lexical

```text
ident       := (letter | "_") (letter | digit | "_")*
string      := '"' (character | escape)* '"'
escape      := "\n" | "\t" | "\r" | "\\" | "\""
comment     := "#" to end of line
```

[Text](/language/02-common-concepts/01-text/) covers strings, escapes, and the
absence of numeric literals.

## Notes on the Grammar

- `module` may appear once, must be the first construct in the file, and is
  a compile error in the entry file.
- Imports come before declarations. Every declaration and every statement
  ends with `;`, including a braced block used as a statement:
  `fn ... { ... };`, `switch(...) { ... };`, `defer { ... };`.
- A `path` may open with `::` to name the root namespace: `::value` and
  `::func()`.
- A `return` is parenthesized: `return();` ends a function with no return
  kind, and `return(expr);` ends a `-> txt` or `-> rec` function. It may
  appear anywhere in its function except inside a `defer` body.
- `panic;` is a keyword statement that ends the run.
- `async <call>;` spawns a call on its own thread and binds nothing; `wait;`
  joins the asyncs the calling thread spawned.
- A `rec` binding takes an expression that is a record literal, a record
  variable, or a call returning a record.
- A binding is immutable unless written `mut`. A `mut` binding may be
  reassigned and, for a record, have its fields assigned; `name.field = expr;`
  replaces one field. A top-level module value can never be `mut`.
- Each parameter declares its kind before its name, optionally preceded by
  `mut`: `txt` is text and `rec` is a record. A function's return kind is
  written `-> txt` or `-> rec` after the parameter list; no arrow means it
  returns no value. `main` takes at most one parameter, and it must be `rec`.
- A bare statement must be a call; a lone value is a compile error.
- A term with no arguments after `.` is a field access.
- The reserved words are `module`, `import`, `fn`, `txt`, `rec`, `mut`,
  `switch`, `case`, `default`, `defer`, `return`, `panic`, `async`, and `wait`.
