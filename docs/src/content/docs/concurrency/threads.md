---
title: Threads
description: Running a call on its own thread with async, and joining it with wait.
---

Kiru can run a function on its own thread while the rest of the program
continues. This is useful for work that should happen in parallel, such as
building two projects at once.

## Starting a thread

`async <call>;` starts a call on its own thread:

```kiru
use std;
use std::text;

fn work() {
  std::command("echo working").run();
};

fn main() {
  async work();
  wait;
  std::command("echo done").run();
};
```

```console
$ ./app
working
done
```

The call's arguments are evaluated at the statement, in the current body,
before the thread starts.

## No handles

Each `async` starts one thread and binds nothing. There is no handle to store,
so a thread cannot be bound, passed, or returned; a call that returns no value
is likewise not a value:

```console
$ kc main.kiru
main.kiru:4:16: error: expected text, found no value
  let x: txt = work();
               ^^^^^^
```

`async` takes one call: a user function, a native call such as
`std::command("cargo build").run()`, or a call through a function value. It
cannot contain another `async`, because `async` is a statement, not a call:

```kiru
use std;
use std::text;

fn main() {
  async std::command("echo parallel").run();
  wait;
};
```

## Waiting for threads

`wait;` joins the threads the calling thread started, then continues:

```kiru
use std;
use std::text;

fn build(name: txt) {
  std::command("echo building @(name)").run();
};

fn main() {
  async build("frontend");
  async build("backend");
  wait;
  std::print("both builds finished");
};
```

A thread started by `async` that runs `wait;` joins only its own children,
usually none, and returns at once. A second `wait;` returns at once when the
calling thread has nothing running.

When `main` returns, the runtime waits for every running thread before it uses
`main`'s return value as the exit status. A thread the program never joins
still runs to completion.

## Shared output

Every thread writes to the same stdout and stderr, so output from parallel work
interleaves. The order between threads is not fixed; writes within one thread
stay in order. A thread that should stay quiet hides a stream on its commands:

```kiru
use std;
use std::text;

fn check() {
  std::command("echo noise").stream("stdout", "/dev/null").run();
};

fn main() {
  async check();
  wait;
  std::print("checks finished");
};
```

```console
$ ./app
checks finished
```

A `panic;` in any thread stops the whole program.
[Panic](/failure/panic/) covers it.

:::note
Threads are OS threads. Each runs a function body with its own scope, and sees
only its arguments and the top-level values.
:::
