---
title: Threads
description: Starting a function on its own thread, joining threads, and failure.
---

`async <call>;` starts `call` on its own thread as a statement:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn work() {
  std::command({}, "echo working");
};

fn main() {
  async work();
  wait;
  std::command({}, "echo done");
};
```

```console
$ ./app
done
```

The call's arguments are evaluated at the statement, in the current body,
before the thread starts. `wait;` takes no argument and joins the asyncs the
calling thread spawned; [the
terminal](#only-the-entry-thread-owns-the-terminal) explains why the first
command writes nothing.

## No Handles

Each `async` starts exactly one thread and yields nothing, and there is no
handle to store. The spawn stands alone, so a thread cannot be bound, passed,
returned, or kept in a record. There is no `std::async` function and no
`std::thread` namespace:

```console
$ kc main.kiru
main.kiru:6:3: error: unknown name `std::async`
  std::async(work());
  ^^^^^^^^^^^^^^^^^
```

A nothing function called as an ordinary expression is still `nothing`, so it
cannot be bound either:

```console
$ kc main.kiru
main.kiru:5:11: error: expected text, found nothing
  txt x = work();
          ^^^^^^
```

`async` takes one call: a user function, or a call such as
`std::command({}, "cargo build")`. It cannot contain another `async`, because
`async` is a statement, not a call. A module value cannot be a thread either,
because a spawn binds nothing; [module
values](/language/03-names-and-scope/04-module-values/) covers that kind
mismatch.

```kiru
fn main() {
  async std::command({}, "cargo build");
  wait;
};
```

A bare `async work();` statement is legal; the runtime joins the thread before
the program exits either way. [Waiting](#waiting) marks where the work is
needed.

## Only the Entry Thread Owns the Terminal

Only the thread running `main` writes to the user's terminal. A thread started
by `async` does not own it, so every command it runs has its stdout and stderr
discarded. `std::print` and `std::eprint` are commands too, so they are silent
on an async thread. A task reports its result through the exit code, through a
file it writes, or through a message the entry thread prints after `wait;`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn check() {
  std::command({}, "echo hidden");   # discarded
};

fn main() {
  async check();
  wait;
  std::print("checks finished");     # shown
};
```

```console
$ ./app
checks finished
```

This is what keeps parallel output from interleaving: the terminal shows only
what the entry thread writes.

## Waiting

`wait;` joins the asyncs the calling thread spawned and then continues. A
thread started by `async` that runs `wait;` joins only its own children,
usually none, and returns at once; the asyncs of other threads are untouched.
A second `wait;` returns at once when the calling thread has nothing running.
At the end of the run, after the entry body ends, the runtime joins every
thread still running, whatever spawned it, before it decides the exit code. A
thread the program never joins still runs to completion, and a failure in it
is still observed.

## Failure

A `panic;` in any thread, including one started with `async`, makes the run
exit nonzero. The panic unwinds the body that raised it and leaves every other
body running; [Defer](/effects/08-failure-and-cleanup/03-defer/) covers the
cleanup an unwind runs. When the entry body ends, the runtime joins the
remaining threads and exits nonzero. The entry thread reports the failure
after `wait;`.

:::note
Threads are OS threads. Each one runs a function body with its own scope.
There is no shared local state between threads; a thread sees only its
arguments and the module values.
:::
