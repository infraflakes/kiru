---
title: Parameters
description: Declaring a parameter's kind, call checking, and mutability.
---

Each parameter declares its kind before its name: `txt` accepts text and `rec` accepts a record. A call is checked against the declared kind; a disagreement is a compile error at the call. A parameter and a function's return type are the two places a kind is written; every other kind is fixed by the declaration or the expression form, and there is no inference.

A parameter is immutable unless declared `mut`. A `mut` parameter is a mutable copy: the function may reassign it and assign its fields, and the caller's value is unaffected. [Assignment and mutability](/language/02-common-concepts/08-assignment/) covers the rules.

A function may return a parameter:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn identity(txt value) -> txt {
  return(value);
};

fn main() {
  std::print(identity("x"));
};
```
