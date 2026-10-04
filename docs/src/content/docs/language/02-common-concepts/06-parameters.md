---
title: Parameters
description: Declaring a parameter's kind, call checking, and read-only bindings.
---

Each parameter declares its kind before its name: `txt` accepts text and `rec` accepts a record. Parameters are read-only bindings, and a call is checked against the declared kind; a disagreement is a compile error at the call. A parameter is the only place a kind is written, and every other kind is fixed by the declaration or the expression form; there is no inference.

A parameter cannot be assigned; [assignment](/language/02-common-concepts/08-assignment/) covers the rule and the workaround of declaring a `txt`.

A function may return a parameter:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn identity(txt value) {
  return(value);
};

fn main() {
  std::print(identity("x"));
};
```
