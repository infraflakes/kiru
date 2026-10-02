---
title: Runtime Builtins
description: The builtins provided by the runtime.
---

The runtime provides these names directly:

```text
std::run(text) -> record { out, code }   run one command line
std::async(call) -> nothing               run a call on a new thread
std::wait() -> nothing                    join the calling thread's asyncs
```

## std::run

`std::run` takes one command line, runs it through the shell, streams stdout
live, and returns a record with the captured stdout in `out` and the exit
code in `code`. A record from a call is read through its fields:

```kiru
fn main() {
  txt tag = std::run("git describe --always").out;
  txt code = std::run("make").code;
  std::print(tag + " " + code);
};
```

The line is passed to a shell, so quoting, pipes, and redirection are the
shell's business. The record-driven `std::command` is the usual way to run a
command; [the command spec](/effects/06-commands/02-builders/) describes it.

## std::async

`std::async(call)` evaluates the invocation's arguments in the current body,
starts it on a new thread, and yields nothing. The argument is any call: a
user function, a native, or a call such as
`std::command({ Mode = "stdout" }, "echo hi")`. It is a statement and may
appear only inside a function body.

```kiru
fn work(txt label) {
  std::command({ Mode = "stdout" }, "echo " + label);
};

fn main() {
  std::async(work("a"));
  std::async(work("b"));
  std::wait();
};
```

There is no handle: the spawn stands alone, and `std::wait` joins the asyncs
the calling thread spawned. [Threads](/effects/07-threads/01-threads/) covers
the rules.

## std::wait

`std::wait()` takes no argument and joins the asyncs the calling thread
spawned, then continues. A thread started by `std::async` that calls
`std::wait()` joins only its own children, usually none, and returns at once;
the asyncs of other threads are untouched. Its result is the `nothing` kind:
it can stand only as a discarded statement. At the end of the run, after the
entry body ends, the runtime joins every thread still running, whatever
spawned it.

```kiru
fn main() {
  std::wait();
};
```

## panic

`panic;` is a keyword statement. It records that the run failed and unwinds
the body that ran it; the run exits nonzero once every thread joins. The
message-carrying form is `std::eprint`, which is Kiru source built on top of
`panic`.
