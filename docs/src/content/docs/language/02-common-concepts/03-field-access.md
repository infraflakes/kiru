---
title: Field Access
description: Reading record fields, and the missing-key rule.
---

`expr.key` reads a field from a record:

<span class="filename">Filename: src/main.kiru</span>

```kiru
rec backend = { name = "backend", dir = "projects/backend" };

fn describe() {
  return(backend.name + " at " + backend.dir);
};

fn main() {
  std::print(describe());
};
```

`describe` reads two fields and concatenates them, so the program prints
`backend at projects/backend`.

The receiver is always a record, whatever its keys, so every record is read
the same way.

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

The record has one field, `A`, so `env.B` reads the empty string and the
program prints `B is []`. [Command line
arguments](/language/09-entry-and-the-cli/02-the-args-record/) is the main
user of this rule: `args.cmd` and `args.flag` read empty when a word is
absent.

## Field Access Is Checked

The receiver must be a record. Text has no fields:

```console
$ kc main.kiru
main.kiru:1:21: error: expected record, found text
fn main() { txt x = "a".b; };
                    ^^^
```

Because every value's kind is fixed before the binary exists, a field access
on a value that might be text is caught at compile time, not left as a
run-time surprise.

## Records Are the Only Receiver

Functions take records too, and a function reads its documented keys at run
time:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn where(rec repo) {
  return(repo.dir);
};

fn main() {
  std::print(where({ dir = "/tmp" }));
};
```

`where` takes a record and returns the `dir` field of whatever record it is
given. A missing key reads `""`, so a caller that never sets a key gives the
function an empty field rather than failing.
