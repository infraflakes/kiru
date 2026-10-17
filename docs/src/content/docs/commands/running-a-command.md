---
title: Running Commands
description: The command builder, run and capture, streams, and the argv-level calls.
---

Running other programs is what Kiru is for. `std::command(line)` starts a
builder that runs one shell line, and `.run()` or `.capture()` finishes it.
`std::process::spawn` and `wait_for` are the argv-level calls underneath.

## The builder

`std::command(line)` returns a `Command` under construction. The line is shell
syntax, run through `/bin/sh -c`, and each setting method returns a new
`Command`, so calls chain:

| Method | Effect |
| --- | --- |
| `dir(path)` | run after `cd <path> &&`, with `path` quoted by `std::quote` |
| `env(name, value)` | prepend `export <name>=<quoted value>;`, with `value` quoted by `std::quote` |
| `timeout(seconds)` | bound the line with `timeout <seconds> sh -c` |
| `stream(stream, target)` | attach `stdin`, `stdout`, or `stderr` to `target` |

An unset method is the default: no directory change, no added environment, no
timeout, and every stream inherited. The options are the private fields of the
`Command` rec (`dir`, `env`, `timeout`, `stdin`, `stdout`, `stderr`), and an
unset entry is the absence, not empty text. `Command` is a `pub` type, so a
program can name and hold a half-built command and configure it conditionally;
the fields are private, so a program sets them through the methods and never by
a field.

```kiru
use std;
use std::text;

fn main() {
  let where: std::Output = std::command("pwd").dir("/tmp").capture();
  std::print(where.out);
  let secret: std::Output = std::command("printenv FOO").env("FOO", "bar").capture();
  std::print(secret.out);
};
```

Holding a command by name is what makes conditional configuration possible: add
a setting only when a flag is set.

```kiru
use std;
use std::text;

fn main() {
  let quiet: bool = true;
  let step: std::Command = std::command("echo working");
  match quiet {
    true => { step = step.stream("stdout", "/dev/null"); };
    _ => {};
  };
  std::print("exit @(step.run())");
};
```

A value that must not become shell syntax is quoted with `std::quote`.

## Finishing a command

`.run()` runs the command and returns its exit code as an `int`:

```kiru
use std;
use std::text;

fn run(line: txt) {
  let code: int = std::command(line).run();
  match code {
    0 => { std::print("@(line) passed"); };
    _ => { std::eprint("@(line) failed with code @(code)"); };
  };
};

fn main() {
  run("true");
  run("false");
};
```

A command's exit code is ordinary data. An unobserved code fails nothing; a
program decides when a nonzero code means failure, as `run` does above by
matching the code and reporting a failure with `std::eprint`.

`.capture()` runs the command and returns an `Output` with the exit code and both
streams as fields `code`, `out`, and `err`. The line's stdout and stderr go to
temporary files, which are read once the command exits, so a large output never
blocks on a pipe and nothing streams live:

```kiru
use std;
use std::text;

fn main() {
  let result: std::Output = std::command("echo hello; echo oops >&2; exit 4").capture();
  match result.code {
    0 => { std::print(result.out); };
    _ => { std::eprint(result.err); };
  };
};
```

`capture` collects the whole line: a shell line of several commands returns all
of their output together.

## Streams

A command's stdout and stderr are inherited and print live by default. Hiding
one keeps the other:

| Call | Effect |
| --- | --- |
| (none) | the stream is inherited and prints live |
| `.stream("stdout", "/dev/null")` | stdout is discarded |
| `.stream("stderr", "/dev/null")` | stderr is discarded |

Hiding stdout while keeping stderr is the usual choice for a parallel step: its
stdout is noise, but a failure still prints its diagnostics.
[Threads](/concurrency/threads/) shows the parallel case.

## argv-level processes

When a shell is not wanted, `std::process::spawn(argv)` runs an argv directly in
its own process group and returns a pid as an `int`. `std::process::wait_for(pid)`
reaps it and returns its exit code as an `int`, and
`std::process::signal(pid, name)` sends a named signal to its group:

```kiru
use std;
use std::text;
use std::vec;
use std::process;

fn main() {
  let pid: int = std::process::spawn(["sleep", "5"]);
  std::process::signal(pid, "KILL");
  let code: int = std::process::wait_for(pid);
  std::print("code: @(code)");
};
```

`spawn` takes an argv, not a shell line, so it sets no directory, environment,
timeout, or streams; every stream is inherited. Use the builder when a command
needs any of those.

`command` is the one way to run a shell line; `spawn` and `wait_for` are the
argv-level primitives underneath it.

A spawn that fails, and exceeding the concurrent-process limit, are runtime
errors. [Panic and Failure](/failure/panic/) covers runtime errors.
