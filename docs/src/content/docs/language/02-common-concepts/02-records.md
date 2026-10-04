---
title: Records
description: Construction, duplicate keys, immutability, and text fields.
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

There is no field assignment. `rec.key = expr` is not a statement:

```console
$ kc main.kiru
main.kiru:3:11: error: expected `;` after the expression, found `=`
  env.FOO = "bar";
          ^
```

To change a record, build a new one at the point of use; [assignment](/language/02-common-concepts/08-assignment/) covers re-binding a whole value.
