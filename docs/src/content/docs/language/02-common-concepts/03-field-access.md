---
title: Field Access
description: Reading record fields, and the missing-key rule.
---

`expr.key` reads a field from a record:

<span class="filename">Filename: src/main.kiru</span>

```kiru
rec backend = { name = "backend", dir = "projects/backend" };

fn describe() -> txt {
  return(backend.name + " at " + backend.dir);
};

fn main() {
  std::print(describe());
};
```

## Missing Keys Read Empty

A missing key reads as `""`. There is no error and no guard needed:

<span class="filename">Filename: src/main.kiru</span>

```kiru
rec env = { A = "1" };

fn main() {
  std::print("B is [" + env.B + "]");
};
```

```console
$ ./app
B is []
```

[Command line arguments](/language/09-entry-and-the-cli/02-the-args-record/) are the main user of this rule.

## Field Access Is Checked

The receiver must be a record. Text has no fields:

```console
$ kc main.kiru
main.kiru:1:21: error: expected record, found text
fn main() { txt x = "a".b; };
                    ^^^
```

A field access on a value that might be text is caught before the binary exists; [type checking](/language/04-types/03-type-checking/) covers the pass. The receiver is a record whatever its keys, so every record is read the same way.
