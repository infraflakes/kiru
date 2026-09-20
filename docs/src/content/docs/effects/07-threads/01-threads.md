---
title: Threads
description: Starting a function on its own thread, joining threads, and failure.
---

`std::async(call)` starts `call` on its own thread as a statement:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn work() {
  std::command("echo working").stream();
};

fn main() {
  std::async(work());
  std::wait();
  std::command("echo done").stream();
};
```

```console
$ ./app
working
done
```

The call's arguments are evaluated at the statement, in the current body,
before the thread starts. The function then runs on the new thread.
`std::wait()` takes no argument and joins the asyncs the calling thread
spawned, so `done` prints only after `working` finishes.

## No Handles

`std::async` yields nothing, and there is no handle to store. The spawn
stands alone as a statement, so a thread cannot be passed, returned, or kept
in a record. There is no `txt` for it and no `std::thread` namespace:

```console
$ kc main.kiru
main.kiru:6:3: error: unknown name `std::thread::wait`
  std::thread::wait();
  ^^^^^^^^^^^^^^^^^
```

The result of an async statement is `nothing`: it can only be a statement.
It cannot be bound, returned, or passed:

```console
$ kc main.kiru
main.kiru:6:11: error: expected text, found nothing
  txt x = std::async(work());
          ^^^^^^^^^^^^^^^^^^
```

A bare `std::async(work());` statement is legal; the runtime joins the
thread before the program exits either way. [Waiting](#waiting) marks where
the work is needed.

:::tip
Start the threads whose results a later step needs, call `std::wait()` once,
and keep going. Because there are no handles, the wait is a barrier for
every async the calling thread has started so far.
:::

## Many Threads

Each `std::async` starts one thread; output can interleave:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn check_format() {
  std::command("echo format").stream();
};

fn check_lints() {
  std::command("echo lints").stream();
};

fn main() {
  std::async(check_format());
  std::async(check_lints());
  std::wait();
  std::command("echo done").stream();
};
```

The two checks run at the same time, so `format` and `lints` may appear in
either order or interleaved; `done` always prints after `std::wait`
returns.

## What May Be Started

The argument is any call expression or method chain. A user function, a
native such as `std::panic`, and a chain such as
`std::command("cargo build").stream()` are all accepted:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::async(std::command("cargo build").stream());
  std::wait();
};
```

Only a function body may start a thread. A module-level initializer cannot
call `std::async`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn work() {
  std::command("echo working").stream();
};

txt worker = std::async(work());

fn main() {};
```

```console
$ kc main.kiru
main.kiru:5:14: error: `std::async` is only allowed inside a function
txt worker = std::async(work());
             ^^^^^^^^^^
```

## Waiting

`std::wait()` joins the asyncs the calling thread spawned and then
continues. A thread started by `std::async` that calls `std::wait()` joins
only its own children, usually none, and returns at once; the asyncs of
other threads are untouched. A second `std::wait()` returns at once when the
calling thread has nothing running. At the end of the run, after the entry
body ends, the runtime joins every thread still running, whatever spawned
it, before it decides the exit code. A thread the program never joins still
runs to completion, and a failure in it is still observed.

## Failure

A `std::panic` in any thread, including one started with `std::async`, makes
the run exit nonzero. The panic unwinds the body that raised it and runs
that body's defers; it leaves every other body running. When the entry body
ends, the runtime joins the remaining threads and exits nonzero.

:::note
Threads are OS threads. Each one runs a function body, streams its own
commands, and registers its own defers. There is no shared local state
between threads; a thread sees only its arguments and the module values.
:::
