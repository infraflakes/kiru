---
title: Grammar
description: The accepted syntax in one place.
---

A compact grammar of the language. Terminals are quoted; `*` means zero or
more; `?` means optional. A term produces a value; a statement ends with `;`.

```text
file        := item*
item        := use | declaration
use         := "use" path ";"
             | "use" path "::" "{" ident ("," ident)* ","? "}" ";"

declaration := "pub"? (function | binding | rec_declaration | enum_declaration | impl_block)
function    := "fn" ident type_params? "(" params? ")" ("->" type)? block ";"
binding     := "let" ident ":" type "=" expression ";"
rec_declaration  := "rec" ident type_params? "{" (field ("," field)* ","?)? "}" ";"
enum_declaration := "enum" ident type_params? "{" (variant ("," variant)* ","?)? "}" ";"
impl_block  := "impl" ident "{" (pub? function)* "}" ";"
field       := "pub"? ident ":" type
variant     := "pub"? ident ("(" (type ("," type)* ","?)? ")")?
type_params := "<" ident ("," ident)* ","? ">"
params      := param ("," param)* ","?
param       := ident ":" type

type          := function_type | type_path
function_type := "fn" "(" (type ("," type)* ","?)? ")" ("->" type)?
type_path     := path type_args?
type_args     := "<" type ("," type)* ","? ">"
path          := ident ("::" ident)*

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
assignment       := ident "=" expression ";"
field_assignment := ident "." ident "=" expression ";"
for              := "for" block ";"

expression     := logical_or
logical_or     := logical_and ("||" logical_and)*
logical_and    := equality ("&&" equality)*
equality       := comparison (("==" | "!=") comparison)*
comparison     := additive (("<" | "<=" | ">" | ">=") additive)*
additive       := multiplicative (("+" | "-") multiplicative)*
multiplicative := unary (("*" | "/" | "%") unary)*
unary          := ("-" | "!") unary | term
term           := primary postfix*
postfix        := "." ident arguments?                        # field, or method call
primary        := string
                | number
                | "true"
                | "false"
                | "{" (field ("," field)* ","?)? "}"            # record literal (a declared type)
                | "(" expression ")"                            # grouping
                | "[" params? "]" ("->" type)? block           # closure
                | "match" expression "{" arm* "}"
                | path arguments?                               # call by path
arguments   := "(" (expression ("," expression)* ","?)? ")"
field       := ident "=" expression

arm         := pattern "=>" (expression | block) ";"
pattern     := variant_pattern | expression
variant_pattern := path "." ident ("(" (pattern_binding ("," pattern_binding)* ","?)? ")")?
pattern_binding := ident | "_"
```

## Lexical

```text
ident       := (letter | "_") (letter | digit | "_")*
number      := digit+
string      := '"' (character | escape | interpolation)* '"'
interpolation := "@(" expression ")"
escape      := "\n" | "\t" | "\r" | "\e" | "\\" | "\"" | "\@"
comment     := "#" to end of line
```

A file is a namespace: it belongs to its directory, and there is no `mod` or
`import`. [Modules](/concepts/modules/) covers namespaces and `use`, and
[Text](/concepts/text/) covers strings, escapes, and interpolation.
