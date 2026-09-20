---
title: Type Checking
description: Declared parameter kinds, fixed expression kinds, and no inference.
---

Kinds are checked in one validation pass over the program. A parameter's
kind is written at its declaration, and every other kind is fixed by the
expression form it comes from. There is no inference and no fixpoint to
iterate.

## Declared Parameters

A parameter is `txt` or `rec`, and every call is checked against that
declared kind. Here `message` is declared `txt`, so it accepts text:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn announce(txt message) {
  std::print(message);
  return("");
};

fn main() {
  announce("hello");
};
```

A parameter declared `rec` is used as a record, such as with `.env`, which
accepts any record:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn use_env(rec repo) {
  return(std::command("build").env(repo).stream().code());
};
```

Passing a record where text is declared is an error at the call, not at run
time:

```console
$ kc main.kiru
main.kiru:7:12: error: expected text, found record
  announce({ A = "1" });
           ^^^^^^^^^^^
```

Here the call passes a record to `announce`, whose parameter is declared
`txt`.

## Kinds Are Fixed by Expression Forms

Every expression has a kind without inference:

- a string literal is text;
- a record literal is record;
- `left + right` is text, and both sides must be text;
- a field access is text, and its receiver must be a record;
- a call takes the kind of its callee: text or record when the function
  returns, `nothing` when it is void, or `never` for a call such as
  `std::panic`;
- a native or method call takes the kind its registry row declares, so
  `std::wait` is `nothing`, `std::command` is command, and `.out` and
  `.code` are text.

A function that ends with `return(expr);` returns text or record; a
function without `return` is void. There is no unconstrained return to
settle later, and no cycle in the kinds to converge.

## No Runtime Type Errors

A compiled program cannot fail because a value had the wrong kind. Every
`+`, field access, call, and chain was checked before the binary existed.
The only failures left at run time are value and operating-system errors,
listed in [compile-time
checks](/language/09-entry-and-the-cli/03-compile-time-checks/).
