---
title: Grammar
description: The accepted syntax in one place.
---

A compact grammar of the language. Terminals are quoted; `*` means zero or
more; `?` means optional. A term produces a value; a statement ends with `;`.

```text
file        := item*
item        := declaration | module | import | use
module      := "mod" path "{" (declaration | use)* "}" ";"
import      := "import" string ";"
use         := "use" path ";"

declaration          := "pub"? (function | binding | builtin_type | rec_declaration | enum_declaration)
function             := "fn" ident type_params? "(" params? ")" ("->" type)? block ";"
binding              := "let" ident ":" type "=" expression ";"
builtin_type         := "type" ident type_params? ";"                   # a runtime type; the missing body is the marker
rec_declaration      := "rec" ident type_params? "{" (field_decl ("," field_decl)* ","?)? "}" ";"
enum_declaration     := "enum" ident type_params? "{" (variant ("," variant)* ","?)? "}" ";"
field_decl  := "pub"? ident ":" type
variant     := "pub"? ident ("(" (type ("," type)* ","?)? ")")?
type_params := "<" ident ("," ident)* ","? ">"
params      := param ("," param)* ","?
param       := ident ":" type

type          := function_type | type_path
function_type := "fn" "(" (type ("," type)* ","?)? ")" ("->" type)?
type_path     := path type_args?
type_args     := "<" type ("," type)* ","? ">"
path          := "::"? ident ("::" ident)*

block       := "{" statement* "}"
statement   := binding
             | assignment
             | field_assignment
             | index_assignment
             | expression ";"
             | "return" expression? ";"
             | "panic" ";"
             | "async" expression ";"
             | "wait" ";"
             | for
             | "break" ";"
assignment       := ident "=" expression ";"
field_assignment := ident "." ident "=" expression ";"
index_assignment := ident "[" expression "]" "=" expression ";"
for         := "for" (ident "in" expression)? block ";"

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
                | "[" expression "]"                            # index
primary        := string
                | number
                | "true"
                | "false"
                | "{" (field ("," field)* ","?)? "}"            # record literal (a declared type)
                | "[" (expression ("," expression)* ","?)? "]"  # vec literal
                | "(" expression ")"                            # grouping
                | "fn" "(" params? ")" ("->" type)? block       # closure
                | "match" expression "{" arm* "}"
                | path arguments?                               # call by path
arguments   := "(" (expression ("," expression)* ","?)? ")"
field       := ident "=" expression

match       := "{" arm* "}"
arm         := pattern "=>" (expression | block) ";"
pattern     := variant_pattern | expression
variant_pattern := ident ("." ident)+ ("(" (pattern_binding ("," pattern_binding)* ","?)? ")")?
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

[Text](/concepts/text/) covers strings, escapes, and interpolation, and
[Values](/concepts/values/) covers the operators.
