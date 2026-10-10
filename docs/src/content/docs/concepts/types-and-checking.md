---
title: Types and Checking
description: The three types, how each is bound, and what the compiler checks.
---

Kiru has three types, and the compiler checks them before your program is
built. In this section we'll meet the three types and see how the compiler
keeps them straight, so a wrong value can never reach a running program.

Every value has one of three types:

```text
txt     string data
rec     a map of names to text
list    an ordered sequence of text
```

All three are data. A call that returns no value produces no value at all; it
may only stand as a statement.

The keywords are `txt`, `rec`, and `list`. Text is the only scalar: numbers,
versions, paths, and flags are all text.

```kiru
let name<txt> = "backend";                          # text
let env<rec> = { RUST_BACKTRACE = "1" };            # record
let backend<rec> = { name = "backend", dir = "." }; # a record with keys
let tags<list> = ["api", "prod"];                   # a list
```

## Binding forms

A binding writes `let`, an optional `mut`, the name, and the type in angle
brackets: `let name<txt> = value;`. Each type has one binding form:

| Type | Annotation | Bound value |
| --- | --- | --- |
| text | `<txt>` | a literal, `+`, a field access, a name, or a call returning text |
| record | `<rec>` | a record literal, a record variable, or a call returning a record |
| list | `<list>` | a list literal, a list variable, or a call returning a list |

A call that returns no value may only stand as a statement. Binding, passing,
storing, or returning such a call is an error.

## What the compiler checks

The compiler checks every expression's type before the program is built. There
is no type inference; each type comes from the declaration or the expression
form:

- a string literal is text;
- a record literal is record;
- `left + right` is text, and both sides must be text;
- a field access is text, and its receiver must be a record;
- a call has the type the callee declares, or no value;
- a parameter's type is written in angle brackets at its declaration, and every
  call is checked against it;
- a function's return type is `-> txt`, `-> rec`, `-> list`, or absent.

Types are exact: a value fits a position only when the types are equal. There
is no subtyping and no group of types.

A disagreement is an error at the call or expression:

```console
$ kc main.kiru
main.kiru:7:12: error: expected text, found record
  announce({ A = "1" });
           ^^^^^^^^^^^
```

A compiled program cannot fail because a value had the wrong type: every `+`,
field access, and call was checked before the binary existed.
