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
before the thread starts. The function then runs on the new thread. `wait;`
takes no argument and joins the asyncs the calling thread spawned, so `done`
prints only after `working` finishes. `work`'s own command writes nothing,
because a thread started by `async` does not own the terminal; [the
terminal](#only-the-entry-thread-owns-the-terminal) explains that.

## No Handles

`async` is a keyword statement and yields nothing, and there is no handle to
store. The spawn stands alone, so a thread cannot be bound, passed, returned,
or kept in a record. There is no `std::async` function and no `std::thread`
namespace:

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

A bare `async work();` statement is legal; the runtime joins the thread before
the program exits either way. [Waiting](#waiting) marks where the work is
needed.

:::tip
Start the threads whose results a later step needs, run `wait;` once, and keep
going. Because there are no handles, the wait is a barrier for every async the
calling thread has started so far.
:::

## Many Threads

Each `async` starts one thread. The two checks run at the same time, but only
the entry thread owns the terminal, so neither check prints anything, and
`done` always prints after `wait` returns:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn check_format() {
  std::command({}, "echo format");
};

fn check_lints() {
  std::command({}, "echo lints");
};

fn main() {
  async check_format();
  async check_lints();
  wait;
  std::command({}, "echo done");
};
```

```console
$ ./app
done
```

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

## What May Be Started

`async` takes one call. A user function and a call such as
`std::command({}, "cargo build")` are both accepted:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  async std::command({}, "cargo build");
  wait;
};
```

An `async` cannot contain another `async`: `async` is a statement, not a call.
A module value cannot be a thread either, because a spawn yields nothing and a
value binding needs data:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn work() {
  std::command({}, "echo working");
};

txt worker = work();

fn main() {};
```

```console
$ kc main.kiru
main.kiru:5:14: error: expected text, found nothing
txt worker = work();
             ^^^^^^
```

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
exit nonzero. The panic unwinds the body that raised it and runs that body's
defers; it leaves every other body running. When the entry body ends, the
runtime joins the remaining threads and exits nonzero. A panic on an async
thread prints nothing, because that thread does not own the terminal; the
entry thread reports the failure after `wait;`.

:::note
Threads are OS threads. Each one runs a function body, does not own the
terminal, and registers its own defers. There is no shared local state
between threads; a thread sees only its arguments and the module values.
:::
