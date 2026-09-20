---
title: Runtime Builtins
description: The builtins provided by the runtime.
---

The runtime provides these names directly:

```text
std::command(text) -> command     a chain from one command line
std::async(call) -> nothing       run a call on a new thread
std::wait() -> nothing            join the calling thread's asyncs
std::panic() -> never             record failure, run defers, exit nonzero
```

## std::command

`std::command` takes a command line and returns a command chain. It runs
nothing and has no side effects; the command runs when a terminal, a
`.stream` statement, or a standalone statement evaluates the chain.

The line is passed to a shell, so quoting, pipes, and redirection are the
shell's business. A value that should not be interpreted by the shell
belongs in `.env`, as the shipped library does for messages; [written by
Kiru](/stdlib/02-written-by-kiru/) shows the pattern.

## std::async

`std::async(call)` evaluates the invocation's receiver and arguments in the
current body, starts it on a new thread, and yields nothing. The argument is
any call expression or method chain: a user function, a native such as
`std::panic`, or a chain such as `std::command("x").code()`. It is a
statement and may appear only inside a function body.

```kiru
fn work(txt label) {
  std::command("echo " + label).stream();
};

fn main() {
  std::async(work("a"));
  std::async(work("b"));
  std::wait();
};
```

There is no handle: the spawn stands alone, and `std::wait` joins the
asyncs the calling thread spawned. [Threads](/effects/07-threads/01-threads/)
covers the rules.

## std::wait

`std::wait()` takes no argument and joins the asyncs the calling thread
spawned, then continues. A thread started by `std::async` that calls
`std::wait()` joins only its own children, usually none, and returns at
once; the asyncs of other threads are untouched. Its result is the
`nothing` kind: it can stand only as a discarded statement. At the end of
the run, after the entry body ends, the runtime joins every thread still
running, whatever spawned it.

```kiru
fn main() {
  std::wait();
};
```

## std::panic

`std::panic` records that the run failed and unwinds the body that called
it; the run exits nonzero once every thread joins. It takes no arguments.
The message-carrying form is `std::eprint`, which is Kiru source built on
top of `panic`.
