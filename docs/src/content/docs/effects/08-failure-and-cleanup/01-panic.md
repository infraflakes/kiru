---
title: Panic
description: panic, std::eprint, and the shape of a failing run.
---

`panic;` records that the run failed and unwinds the body that ran it:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn give_up() {
  panic;
};
```

`panic;` is a statement, not a value: it ends the run. A function whose body
ends in it does not fall through, whatever return kind it declares.

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

A panic is not an immediate exit. The body that raised it records the failure
so the run exits nonzero, then unwinds. The failure is global, but the unwind
is local: it never signals another body's process groups, so a panic in one
thread leaves the other threads running. When the entry body ends, the
runtime joins every thread and then decides the exit code. A runtime error the
engine detects, such as a command that cannot start, records the same failure.

Cleanup registered for a body runs on the panic path.
[Defer](/effects/08-failure-and-cleanup/03-defer/) covers the order, and a
panic inside one is reported while the remaining cleanup still runs.

A nonzero exit code by itself is data; `panic` is how a program declares
failure. [Signals](/effects/08-failure-and-cleanup/02-signals/) covers the
events that stop the running process groups.
