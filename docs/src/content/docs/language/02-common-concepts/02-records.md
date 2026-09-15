---
title: Records
description: Construction, duplicate keys, text fields, and field assignment.
---

A record is a map of names to text:

<span class="filename">Filename: src/main.kiru</span>

```kiru
rec env = {
  RUST_BACKTRACE = "1",
  CARGO_TERM_COLOR = "never",
};
```

Keys are identifiers. A trailing comma is allowed, `{}` is the empty record, and a later duplicate key wins:

```kiru
rec flags = { a = "1", a = "2" };    # a is "2"
```

Every value in a record is a text expression. Records do not nest and every field is text:

```console
$ kc main.kiru
main.kiru:1:24: error: expected text, found record
rec nested = { inner = { a = "1" } };
                       ^^^^^^^^^^^
```

Because every field is text, `rec.key` is always text, and a missing key reads as `""`; [field access](/language/02-common-concepts/03-field-access/) covers that read.

A record is immutable unless its binding is `mut`. A field assignment `rec.key = expr` replaces one field of a mutable record, and adds the field when it is absent:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  mut rec env = { RUST_BACKTRACE = "1" };
  env.RUST_BACKTRACE = "0";
  env.CARGO_TERM_COLOR = "never";
};
```

Assigning a field of a binding that is not `mut` is a compile error; [assignment and mutability](/language/02-common-concepts/08-assignment/) covers the rules.
