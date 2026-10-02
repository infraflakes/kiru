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
function    := "fn" ident "(" params? ")" block ";"
text        := "txt" ident "=" expression ";"
record      := "rec" ident "=" fields ";"

params      := kind ident ("," kind ident)* ","?
kind        := "txt" | "rec"
fields      := "{" (field ("," field)* ","?)? "}"
field       := ident "=" expression
path        := "::"? ident ("::" ident)*

block       := "{" statement* "}"
statement   := text
             | record
             | assignment
             | expression ";"
             | "return" expression? ";"
             | "panic" ";"
             | "switch" "(" expression ")" switch ";"
             | "defer" block ";"
assignment  := ident "=" expression ";"

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

Strings may span lines. Any escape other than the five listed is a compile
error. There are no numeric literals.

## Notes on the Grammar

- `module` may appear once, must be the first construct in the file, and is
  a compile error in the entry file.
- Imports come before declarations. Every declaration and every statement
  ends with `;`, including a braced block used as a statement:
  `fn ... { ... };`, `switch(...) { ... };`, `defer { ... };`.
- A `path` may open with `::` to name the root namespace: `::value` and
  `::func()`.
- A `return` has no parentheses: `return;` ends a void function, and
  `return expr;` returns a value. It may appear anywhere except inside
  `defer`.
- `panic;` is a keyword statement that ends the run.
- Each parameter declares its kind before its name: `txt` is text and `rec`
  is a record. `main` takes at most one parameter, and it must be `rec`.
- A bare statement must be a call; a lone value is a compile error.
- A term with no arguments after `.` is a field access.
- The reserved words are `module`, `import`, `fn`, `txt`, `rec`, `switch`,
  `case`, `default`, `defer`, `return`, and `panic`.
