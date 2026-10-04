---
title: Type Checking
description: Declared parameter kinds, fixed expression kinds, and no inference.
---

Kinds are checked in one validation pass over the program. A parameter's kind is written at its declaration, and every other kind is fixed by the expression form it comes from. There is no inference and no fixpoint to iterate.

A parameter is `txt` or `rec`, and every call is checked against that declared kind. Here `message` is declared `txt`, so it accepts text:

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

Passing a record where text is declared is an error at the call, not at run time:

```console
$ kc main.kiru
main.kiru:7:12: error: expected text, found record
  announce({ A = "1" });
           ^^^^^^^^^^^
```

Every expression has a kind without inference:

- a string literal is text;
- a record literal is record;
- `left + right` is text, and both sides must be text;
- a field access is text, and its receiver must be a record;
- a call takes the kind of its callee: text or record when the function returns, or `nothing` when it returns no value;
- a native call takes the kind declared for it; [the runtime builtins](/stdlib/01-runtime-builtins/) lists them.

A function whose returns share one kind has that kind; a function with no return, or whose returns have no common kind, is `nothing`. [Return and nothing](/language/02-common-concepts/07-return-and-recursion/) covers `nothing`.

A compiled program cannot fail because a value had the wrong kind: every `+`, field access, and call was checked before the binary existed.
