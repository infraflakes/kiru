---
title: Calls
description: Expression statements, and discarded results.
---

A statement can be a call:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn work() {
  return("");
};

fn main() {
  std::print("hello");
  work();
};
```

A bare expression statement must be a call: a function call and a native call are both legal, and `panic;` stands bare as well. A literal, a variable, a record literal, or a concatenation as a statement is a compile error:

```console
$ kc main.kiru
main.kiru:2:3: error: a statement must be a call
  "x";
  ^^^
```

The result of a call statement is discarded, and nothing warns about a discarded value. A call to a function that returns nothing has the `nothing` kind, so it may stand only as a statement; [return and nothing](/language/02-common-concepts/07-return-and-recursion/) covers the diagnostics.

In a call, arguments are evaluated left to right before the call:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt version = "1";

fn main() {
  std::print("version " + version);
};
```

Record-driven commands are covered by [the command spec](/effects/06-commands/02-builders/), and spawning work is covered by [threads](/effects/07-threads/01-threads/).
