---
title: Defers During Unwind
description: The order defers run in when a body fails.
---

When a panic unwinds a body, that body's defers run before the panic
continues outward. Each enclosing body runs its defers as the panic passes
through, so every registered cleanup runs exactly once.

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn inner() {
  defer { std::print("inner cleanup"); };
  std::panic();
};

fn outer() {
  defer { std::print("outer cleanup"); };
  inner();
};

fn main() {
  outer();
};
```

```console
$ ./app
inner cleanup
outer cleanup
$ echo $?
1
```

The innermost defer runs first, then the next body outward, until the panic
reaches the top and the process exits.

## LIFO Within a Body

Within one body, defers run in reverse declaration order:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn body() {
  defer { std::print("first"); };
  defer { std::print("second"); };
  std::panic();
};
```

```console
$ ./app
second
first
```

The stack order is the whole rule. There is no priority, no condition, and
no way to skip a defer.

## Threads

A thread started by `std::async` runs a function body with its own defers;
they run when that body ends. A function that starts a thread and wants that
thread's cleanup finished before it returns calls `std::wait()` first. The
runtime also joins every remaining thread before the program exits.

## A Defer That Panics

A panic inside a defer is reported, and the remaining defers still run:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn messy() {
  defer { std::print("still runs"); };
  defer { std::panic(); };
};
```

```console
$ ./app
still runs
$ echo $?
1
```

The second defer panics; the first still runs. The process exits nonzero; a
panic during cleanup does not change the fact that the run failed.
