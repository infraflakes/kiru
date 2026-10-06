---
title: Grammar
description: The accepted syntax in one place.
---

A compact grammar of the language. Terminals are quoted; `*` means zero or
more; `?` means optional.

```text
file        := module? import* declaration*
module      := "module" path ";"
import      := "import" string ";"

declaration := function | text | record | list
function    := "fn" ident "(" params? ")" ("->" type)? block ";"
text        := "mut"? "txt" ident "=" expression ";"
record      := "mut"? "rec" ident "=" expression ";"
list        := "mut"? "list" ident "=" expression ";"

params      := param ("," param)* ","?
param       := "mut"? type ident
type        := "txt" | "rec" | "list"
fields      := "{" (field ("," field)* ","?)? "}"
field       := ident "=" expression
path        := "::"? ident ("::" ident)*

block       := "{" statement* "}"
statement   := text
             | record
             | list
             | assignment
             | field_assignment
             | expression ";"
             | "return" "(" expression? ")" ";"
             | "panic" ";"
             | "async" expression ";"
             | "wait" ";"
             | "switch" "(" expression ")" switch ";"
             | for
             | "break" ";"
assignment  := ident "=" expression ";"
field_assignment := ident "." ident "=" expression ";"
for         := "for" (ident "in" expression)? block ";"

switch      := "{" (case | default)* "}"
case        := "case" "(" expression ")" block ";"
default     := "default" block ";"

expression  := term ("+" term)*
term        := primary ("." ident)*
primary     := string
             | fields
             | list_literal
             | path arguments?
             | path
arguments   := "(" (expression ("," expression)* ","?)? ")"
list_literal := "[" (expression ("," expression)* ","?)? "]"
```

## Lexical

```text
ident       := (letter | "_") (letter | digit | "_")*
string      := '"' (character | escape)* '"'
escape      := "\n" | "\t" | "\r" | "\e" | "\\" | "\""
comment     := "#" to end of line
```

[Text](/concepts/text/) covers strings, escapes, and the absence of numeric
literals.

## Notes

- `module` may appear once, must be the first construct in the file, and is an
  error in the entry file.
- Imports come before declarations. Every declaration and every statement ends
  with `;`, including a braced block used as a statement: `fn ... { ... };`,
  `switch(...) { ... };`, `for { ... };`.
- A `path` may open with `::` to name the root namespace: `::value` and
  `::func()`.
- A `return` is parenthesized: `return();` ends a function with no return type,
  and `return(expr);` ends a `-> txt`, `-> rec`, or `-> list` function.
- `panic;` exits the program with an error.
- A case pattern is a literal, a name, or a field path, not any expression.
- `async <call>;` starts a call on its own thread and binds nothing; `wait;`
  joins the asyncs the calling thread spawned.
- A `rec` binding takes a record literal, a record variable, or a call
  returning a record; a `list` binding takes a list literal, a list variable, or
  a call returning a list.
- A binding is immutable unless written `mut`. A `mut` binding may be
  reassigned and, for a record, have its fields assigned; `name.field = expr;`
  replaces one field. A top-level value can never be `mut`.
- Each parameter declares its type before its name, optionally preceded by
  `mut`. A function's return type is written `-> txt`, `-> rec`, or `-> list`
  after the parameter list; no arrow means it returns no value. `main` takes at
  most one parameter, and it must be `rec`.
- `for` has two shapes: `for item in <list> { ... };` runs the body once per
  element, and `for { ... };` repeats until a `break;`. `break;` ends the
  nearest loop and is an error outside a loop.
- A bare statement must be a call; a lone value is an error.
- A term with no arguments after `.` is a field access.
- The reserved words are `module`, `import`, `fn`, `txt`, `rec`, `list`, `mut`,
  `switch`, `case`, `default`, `return`, `panic`, `async`, `wait`, `for`,
  `in`, and `break`.
