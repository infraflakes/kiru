---
title: Grammar
description: The accepted syntax in one place.
---

A compact grammar of the language. Terminals are quoted; `*` means zero or
more; `?` means optional.

```text
file        := item*
item        := declaration | module | import
module      := "mod" path "{" declaration* "}" ";"
import      := "import" string ";"

declaration := function | binding
function    := "fn" ident "(" params? ")" ("->" type)? block ";"
binding     := "let" "mut"? ident "<" type ">" "=" expression ";"

params      := param ("," param)* ","?
param       := "mut"? ident "<" type ">"
type        := "txt" | "rec" | "list"
fields      := "{" (field ("," field)* ","?)? "}"
field       := ident "=" expression
path        := "::"? ident ("::" ident)*

block       := "{" statement* "}"
statement   := binding
             | assignment
             | field_assignment
             | expression ";"
             | "return" expression? ";"
             | "panic" ";"
             | "async" expression ";"
             | "wait" ";"
             | for
             | "break" ";"
assignment  := ident "=" expression ";"
field_assignment := ident "." ident "=" expression ";"
for         := "for" (ident "in" expression)? block ";"

expression  := term ("+" term)*
term        := primary ("." ident)*
primary     := string
             | fields
             | list_literal
             | "match" expression match
             | path arguments?
             | path
match       := "{" arm* "}"
arm         := pattern "=>" (expression | block) ";"
pattern     := string | "_" | ident ("." ident)*
arguments   := "(" (expression ("," expression)* ","?)? ")"
list_literal := "[" (expression ("," expression)* ","?)? "]"
```

## Lexical

```text
ident       := (letter | "_") (letter | digit | "_")*
string      := '"' (character | escape | interpolation)* '"'
interpolation := "@(" expression ")"
escape      := "\n" | "\t" | "\r" | "\e" | "\\" | "\"" | "\@"
comment     := "#" to end of line
```

[Text](/concepts/text/) covers strings, escapes, and the absence of numeric
literals.

## Notes

- A `mod a::b { ... };` block declares an inline namespace. A file may hold
  several, and reopening a path merges into the same namespace.
- `import "path";` is an item that splices the imported file's items at its
  position; it is global-level only, never inside a `mod`. A file is imported
  once, so importing it twice or in a cycle is an error.
- Every declaration and every statement ends with `;`, including a braced block
  used as a statement: `fn ... { ... };`, `mod ... { ... };`, `match ... { ... };`,
  `for { ... };`.
- A `path` may open with `::` to name the root namespace: `::value` and
  `::func()`.
- A `return` is `return;` to end a function with no return type, and
  `return expr;` to end a `-> txt`, `-> rec`, or `-> list` function.
- `panic;` exits the program with an error.
- A match is a term: `match subject { pattern => body; … };`. In expression
  position its arms are expressions and it yields the taken arm's value; in
  statement position its arms are blocks. A pattern is a literal, a name, or a
  field path, not any expression; the `_` arm is the default.
- A string may interpolate expressions with `@(expr)`; the expression must be
  text, and `\@` writes a literal `@(`.
- `async <call>;` starts a call on its own thread and binds nothing; `wait;`
  joins the asyncs the calling thread spawned.
- A binding is written `let name<txt> = value;`, `let name<rec> = value;`, or
  `let name<list> = value;`. A `rec` binding takes a record literal, a record
  variable, or a call returning a record; a `list` binding takes a list literal,
  a list variable, or a call returning a list.
- A binding is immutable unless written `let mut`: `let mut name<txt> = value;`.
  A `mut` binding may be reassigned and, for a record, have its fields assigned;
  `name.field = expr;` replaces one field. A top-level value can never be `mut`.
- Each parameter declares its name then its type in angle brackets, optionally
  preceded by `mut`: `fn f(a<txt>, b<rec>) -> txt`. A function's return type is
  written `-> txt`, `-> rec`, or `-> list` after the parameter list; no arrow
  means it returns no value. `main` takes at most one parameter, and it must be
  `rec`.
- `for` has two shapes: `for item in <list> { ... };` runs the body once per
  element, and `for { ... };` repeats until a `break;`. `break;` ends the
  nearest loop and is an error outside a loop.
- A bare statement must be a call or a match; a lone value is an error.
- A term with no arguments after `.` is a field access.
- The reserved words are `mod`, `import`, `fn`, `let`, `txt`, `rec`, `list`,
  `mut`, `match`, `return`, `panic`, `async`, `wait`, `for`, `in`, and `break`.
