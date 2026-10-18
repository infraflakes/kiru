---
title: Panic and Failure
description: panic, std::eprint, the runtime errors, and the exit status.
---

A program can fail on purpose, and the runtime can fail on its own. This
chapter covers `panic;`, the runtime errors, and the exit status.

## Panic

`panic;` marks the program as failed and exits `1`. It is a statement, not a
value:

```kiru
fn give_up() {
  panic;
};

fn main() {
  std::print("ready");
};
```

Calling `give_up` stops the whole program: it cancels every other thread and
sends `SIGTERM` to every running process group, so no command a Kiru program
started outlives the failure. [Threads](/concurrency/threads/) covers the
threads.

## Reporting with eprint

`std::eprint(message)` writes an `ERROR:` line to stderr, red when stderr is a
terminal, then panics:

```kiru
use std;

fn report(reason: txt) {
  std::eprint("cannot continue: @(reason)");
};

fn main() {
  std::print("ready");
};
```

```console
$ ./app
ERROR: cannot continue: missing token
$ echo $?
1
```

A function whose every path ends in `panic;` or a call that panics never
returns, so it can stand where a value is expected:

```kiru
use std;

fn report(reason: txt) -> txt {
  std::eprint(reason);
};

fn main() {
  std::print("ready");
};
```

A command's exit code is ordinary data, and a nonzero code fails nothing by
itself. [Running Commands](/commands/running-a-command/) shows how a program
reports a failed command with a `match` on the code and `std::eprint`.

## Runtime errors

The runtime detects some failures the compiler cannot, and reports them the
same way: an `ERROR:` line naming the call, then the run stops.

```text
ERROR: index 2 is outside a Vec of 2 element(s)
ERROR: posix_spawn(...): no-such-program: No such file or directory (os error 2)
ERROR: posix_spawn(...): too many concurrent processes
```

## Exit status

A normal run exits with the status `main` returns. `main` declares `-> int` or
nothing; a returned `int` is the exit code, clamped to `0..=255`, and no return
means `0`:

```kiru
use std;
fn main() -> int {
  return 0;
};
```

```console
$ ./app
$ echo $?
0
```

Returning `2`, for example, would exit `2`. A `panic;` or a runtime error exits
`1`, and a signal exits `130`. [Signals and
Shutdown](/failure/signals/) covers the signals, and [Hello,
World!](/getting-started/hello-world/) covers the `kc` command.
