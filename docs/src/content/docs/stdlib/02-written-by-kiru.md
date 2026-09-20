---
title: Written by Kiru
description: print and eprint, the library written in Kiru.
---

The rest of the standard library is Kiru source embedded in the compiler
and loaded into every program.

```text
std::print(text) -> text           write a line to stdout, returns ""
std::eprint(text) -> nothing       write a line to stderr, then panic; "" silently
```

Both functions live in the `std` namespace, so the unqualified `command`
inside them is `std::command`.

## print

```kiru
fn print(txt message) {
  command("printf '%s\\n' \"$KIRU_MESSAGE\"")
    .env({ KIRU_MESSAGE = message })
    .stream();
  return("");
};
```

`print` returns the empty string, so a call is text and may be composed or
used as a statement. The message travels through the environment, never
through the command line, so its text cannot become shell syntax. A message
containing quotes, `$(...)`, or newlines prints exactly as written.

## eprint

```kiru
fn eprint(txt message) {
  switch(message) {
    case("") {};
    default {
      command("printf '%s\\n' \"$KIRU_MESSAGE\" >&2")
        .env({ KIRU_MESSAGE = message })
        .stream();
    };
  };
  panic();
};
```

`eprint` is void: it has no `return`, and its body ends in `std::panic();`.
An empty message skips the print and panics silently. Otherwise the message
goes to stderr and the run ends. Because the call is void, it may only stand
as a statement.

## Embedding

The library is loaded into every program before the entry file, so its
names are always in `std`. A program that never prints does not carry
`eprint`, because unused library declarations are dropped at compile time.
