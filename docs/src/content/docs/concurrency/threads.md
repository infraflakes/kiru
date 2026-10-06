---
title: Threads
description: Run a call on its own thread and wait for it.
---

Kiru can run a function on its own thread while the rest of the program
continues. This is useful for work that should happen in parallel, such as
building two projects at once.

## Starting a Thread

`async <call>;` starts a call on its own thread:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn work() {
  std::process::command({}, "echo working");
};

fn main() {
  async work();
  wait;
  std::process::command({}, "echo done");
};
```

```console
$ ./app
working
done
```

The call's arguments are evaluated at the statement, in the current body,
before the thread starts.

## No Handles

Each `async` starts one thread and binds nothing. There is no handle to store,
so a thread cannot be bound, passed, or returned:

```console
$ kc main.kiru
main.kiru:5:11: error: expected text, found nothing
  txt x = work();
          ^^^^^^
```

`async` takes one call: a user function or a native call such as
`std::process::command({}, "cargo build")`. It cannot contain another `async`,
because `async` is a statement, not a call:

```kiru
fn main() {
  async std::process::command({}, "cargo build");
  wait;
};
```

## Waiting for Threads

`wait;` joins the threads the calling thread started, then continues:

```kiru
fn main() {
  async build("frontend");
  async build("backend");
  wait;
  std::io::print("both builds finished");
};
```

A thread started by `async` that runs `wait;` joins only its own children,
usually none, and returns at once. A second `wait;` returns at once when the
calling thread has nothing running.

When `main` returns, the runtime waits for every running thread before it uses
`main`'s return value as the exit status. A thread the program never joins
still runs to completion.

## Shared Output

Every thread writes to the same stdout and stderr, so output from parallel work
interleaves. The order between threads is not fixed; writes within one thread
stay in order. A thread that should stay quiet sets `Stdout = "null"` or
`Stderr = "null"` on its commands:

```kiru
fn check() {
  std::process::command({ Stdout = "null" }, "make");
};

fn main() {
  async check();
  wait;
  std::io::print("checks finished");
};
```

```console
$ ./app
checks finished
```

A `panic;` in any thread stops the whole program. [Panic](/failure/panic/)
covers it.

:::note
Threads are OS threads. Each runs a function body with its own scope, and sees
only its arguments and the top-level values.
:::
