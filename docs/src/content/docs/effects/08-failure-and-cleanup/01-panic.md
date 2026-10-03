---
title: Panic
description: panic, std::eprint, and the shape of a failing run.
---

`panic;` records that the run failed and unwinds the body that ran it. The
run continues until the entry body ends; then every thread joins and the
process exits nonzero.

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn give_up() {
  panic;
};
```

`panic;` is a statement, not a value: it ends the run, so the function that
runs it is `nothing` and needs no `return`.

`std::eprint(message)` writes an `ERROR:` line to stderr, red when stderr is a
terminal, and then panics:

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
ERROR: cannot continue: missing token
$ echo $?
1
```

`std::eprint` is `nothing` and ends in `panic;`, so it stands only as a
statement.

## What a Panic Does

A panic is not an immediate `exit`. The body that raised it:

1. records the failure so the run exits nonzero,
2. unwinds, running every pending defer innermost first.

The failure is global, but the panic is local: it never signals another
body's process groups, so a panic in one thread leaves the other threads
running. When the entry body ends, the runtime joins every thread and then
decides the exit code.

:::note
A runtime error the engine detects, such as a command that cannot start,
records the same failure. Ctrl+C, SIGTERM, and SIGHUP are the only events
that stop the running process groups.
:::

## Defers Run

Cleanup registered with `defer` runs on the panic path:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn release() {
  std::command({}, "mkdir -p /tmp/kiru-release");

  defer {
    std::command({}, "rm -rf /tmp/kiru-release");
  };

  std::command({}, "cargo build --release");   # may fail
};
```

If the build fails, the defer still removes the staging directory.

## Nested Panics

A panic inside a defer is reported, and the remaining defers still run.

:::note
A nonzero exit code, by itself, is data; the program decides what it means.
`panic` is how a program declares failure, and runtime errors the
engine detects take the same path.
:::
