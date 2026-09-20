---
title: Exit Codes and Failure
description: Exit codes as text, and how a run fails.
---

An exit code is text. `"0"` means success; anything else is a nonzero code
the program can inspect:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn run(txt line) {
  return(std::command(line).stream().code());
};

fn main() {
  switch(run("true")) {
    case("0") { std::print("ok"); };
    default { std::eprint("command failed"); };
  };
};
```

`run` streams a command and returns its code. `main` prints `ok` for a zero
code and takes the `default` arm for anything else.

## Unobserved Codes Never Fail

A nonzero code by itself does nothing. A run fails when the program calls
`std::panic`, or when the engine reports a runtime error:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("false").stream();     # code 1, ignored
  std::print("still running");
};
```

```console
$ ./app
still running
$ echo $?
0
```

The command exits `1` and the program carries on. Many commands return
nonzero for reasons a program handles: `grep` returns `1` when nothing
matches, and `test -d` returns `1` when a directory is missing.

## Failing on Purpose

A command that should fail the run checks the code and panics:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("false").stream();
  std::eprint("a command failed");
};
```

`std::eprint` writes the message to stderr and then panics. The panic
unwinds the body, runs pending defers, and records the failure; the run
exits nonzero once every thread joins. A helper that runs a command uses
this shape: it streams the command and calls `std::eprint` with the command
when the code is not `"0"`.

## Collecting Codes

A program can run several commands, collect their codes, print a report,
and fail at the end:

```console
$ ./app check
  build: 0
  test: 0
  fmt: 0
```

The report is built as text, printed once, and followed by a failure if any
code was nonzero. Codes are data; the program decides what to do with them.
The common comparison is `"0"` in a `case` and `default` for everything
else.
