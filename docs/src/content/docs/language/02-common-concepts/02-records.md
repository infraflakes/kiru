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

Keys are identifiers. A trailing comma is allowed, and `{}` is the empty
record.

## Duplicate Keys

A later duplicate key wins:

<span class="filename">Filename: src/main.kiru</span>

```kiru
rec flags = { a = "1", a = "2" };    # a is "2"
```

The compiler does not reject the duplicate; the record has one field with
the last value.

## Fields Are Text

Every value in a record is a text expression. Records do not nest and every
field is text:

```console
$ kc main.kiru
main.kiru:1:24: error: expected text, found record
rec nested = { inner = { a = "1" } };
                       ^^^^^^^^^^^
```

```console
$ kc main.kiru
main.kiru:1:24: error: expected text, found record
rec result = { run = std::run("ls") };
                       ^^^^^^^^^^^^^^
```

Both fields are rejected because the value is not text. Because every field
is text, `rec.key` is always text, and a missing key reads as `""`; [field
access](/language/02-common-concepts/03-field-access/) covers that read.

## Records Are Immutable

There is no field assignment. `rec.key = expr` is not a statement:

```console
$ kc main.kiru
main.kiru:3:11: error: expected `;` after the expression, found `=`
  env.FOO = "bar";
          ^
```

To change a record, assign a new record to the binding, or build the record
at the point of use. Records flow into function parameters and into a command
spec, and are otherwise ordinary values.

## Records as Arguments

A record literal may appear anywhere a record is expected:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn build(txt line, rec spec) {
  return std::command(spec, line);
};

fn main() {
  txt code = build("cargo build", { Mode = "exit code" });
  std::print("code " + code);
};
```

The literal is constructed at the call and passed by value. There is no
aliasing and no mutation, so a record handed to a function cannot be
changed by it.
