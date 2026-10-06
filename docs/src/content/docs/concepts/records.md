---
title: Records
description: Record literals, field access, and field assignment.
---

A record groups named pieces of text together. Records show up everywhere in
Kiru: a command's options and a program's arguments are both records. In this
section we'll build one, read its fields, and change a field.

A record is a map of names to text:

<span class="filename">Filename: src/main.kiru</span>

```kiru
rec env = {
  RUST_BACKTRACE = "1",
  CARGO_TERM_COLOR = "never",
};
```

Keys are identifiers. A trailing comma is allowed, `{}` is the empty record,
and a later duplicate key wins:

```kiru
rec flags = { a = "1", a = "2" };    # a is "2"
```

Every field is a text expression, so records do not nest:

```console
$ kc main.kiru
main.kiru:1:24: error: expected text, found record
rec nested = { inner = { a = "1" } };
                       ^^^^^^^^^^^
```

## Field access

`expr.key` reads a field. A missing key reads as `""`, with no error and no
guard:

```kiru
rec env = { A = "1" };
std::io::print("B is [" + env.B + "]");   # B is []
```

The receiver must be a record; a field access on text is an error. Because
every field is text, `expr.key` is always text.

## Field assignment

A field assignment replaces one field of a `mut` record, and adds the field
when it is absent:

```kiru
fn main() {
  mut rec env = { RUST_BACKTRACE = "1" };
  env.RUST_BACKTRACE = "0";
  env.CARGO_TERM_COLOR = "never";
};
```

Assigning a field of a binding that is not `mut` is an error. [Bindings and
Mutability](/concepts/bindings-and-mutability/) covers `mut`.
