---
title: Types and Checking
description: The four types, how each is bound, and what the compiler checks.
---

Kiru has four types, and the compiler checks them before your program is built.
In this section we'll meet the four types and see how the compiler keeps them
straight, so a wrong value can never reach a running program.

Every value has one of four types:

```text
text       string data
record     a map of names to text
list       an ordered sequence of text
nothing    the result of a call that returns no value
```

`text`, `record`, and `list` are data. `nothing` is not a value you can store;
it is only the result of a call that returns no value.

The keywords are `txt`, `rec`, and `list`. Text is the only scalar: numbers,
versions, paths, and flags are all text.

```kiru
txt name = "backend";                              # text
rec env = { RUST_BACKTRACE = "1" };                # record
rec backend = { name = "backend", dir = "." };     # a record with keys
list tags = ["api", "prod"];                       # a list
```

## Binding forms

Each type has one binding form:

| Type | Keyword | Bound value |
| --- | --- | --- |
| text | `txt` | a literal, `+`, a field access, a name, or a call returning text |
| record | `rec` | a record literal, a record variable, or a call returning a record |
| list | `list` | a list literal, a list variable, or a call returning a list |
| nothing | — | a call that returns no value; may only stand as a statement |

Binding, passing, storing, or returning a `nothing` call is an error.

## What the compiler checks

The compiler checks every expression's type before the program is built. There
is no type inference; each type comes from the declaration or the expression
form:

- a string literal is text;
- a record literal is record;
- `left + right` is text, and both sides must be text;
- a field access is text, and its receiver must be a record;
- a call has the type the callee declares, or `nothing`;
- a parameter's type is written at its declaration, and every call is checked
  against it;
- a function's return type is `-> txt`, `-> rec`, `-> list`, or absent.

A disagreement is an error at the call or expression:

```console
$ kc main.kiru
main.kiru:7:12: error: expected text, found record
  announce({ A = "1" });
           ^^^^^^^^^^^
```

A compiled program cannot fail because a value had the wrong type: every `+`,
field access, and call was checked before the binary existed.
