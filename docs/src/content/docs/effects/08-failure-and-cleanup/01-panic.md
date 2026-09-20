---
title: Panic
description: std::panic, std::eprint, and the shape of a failing run.
---

`std::panic` records that the run failed and unwinds the body that called
it. The run continues until the entry body ends; then every thread joins and
the process exits nonzero.

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn give_up() {
  std::panic();
};
```

A call to `std::panic` is `never`: it is a statement and ends the run, so
the function that calls it is void and needs no `return`.

`std::eprint(message)` writes one line to stderr and then panics:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn fail(txt reason) {
  std::eprint("cannot continue: " + reason);
};

fn main() {
  fail("missing token");
};
```

```console
$ ./app
cannot continue: missing token
$ echo $?
1
```

`std::eprint` is void and ends in `std::panic();`, so it stands only as a
statement. `std::eprint("")` panics with no output, for the rare case where
the exit code is the whole message.

## What a Panic Does

A panic is not an immediate `exit`. The body that raised it:

1. records the failure so the run exits nonzero,
2. unwinds, running every pending defer innermost first.

The failure is global, but the panic is local: it never signals another
body's process groups, so a panic in one thread leaves the other threads
running. When the entry body ends, the runtime joins every thread and then
decides the exit code.

:::note
A runtime error the engine detects, such as an empty directory or a command
that cannot start, records the same failure. Ctrl+C, SIGTERM, and SIGHUP
are the only events that stop the running process groups.
:::

## Defers Run

Cleanup registered with `defer` runs on the panic path:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn release() {
  txt scratch = std::command("mktemp -d").out();
  defer { std::command("rm -rf " + scratch).stream(); };
  std::command("cargo build --release").stream();   # may fail
};
```

If the build fails, the defer still removes the staging directory.

## Nested Panics

A panic inside a defer is reported, and the remaining defers still run.

:::note
A nonzero exit code, by itself, is data; the program decides what it means.
`std::panic` is how a program declares failure, and runtime errors the
engine detects take the same path.
:::
