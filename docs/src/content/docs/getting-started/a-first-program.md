---
title: A First Program
description: Build a small task runner and meet the basics of Kiru.
---

This chapter builds a small task runner: a program that runs a named task and
reports whether it succeeded. Along the way it introduces the pieces a real
Kiru program uses — a declared type, `match`, interpolation, the command
builder, and `std::cli`. The chapters after this one explain each piece in
detail.

The finished program reads a task from the command line, runs that task's
command, and reports the result.

## Running a Command

Running other programs is what Kiru is for. `std::process::command(line)` starts
a command under construction, and `.run()` runs it through the shell
(`/bin/sh -c`) and returns the exit code as an `int`:

<span class="filename">Filename: tasks.kiru</span>

```kiru
use std;
use std::process;

fn main() -> int {
  return std::process::command("echo building").run();
};
```

`main` returns the command's exit code as the program's own status. [Running
Commands](/commands/running-a-command/) covers the builder's other options, and
[Panic and Failure](/failure/panic/) defines the exit status.

Compile and run it in a project directory:

```console
$ kc tasks.kiru
$ ./tasks
building
$ echo $?
0
```

## A Declared Type

The runner should name each task, so give a task a shape. `rec` declares a type
with named fields:

<span class="filename">Filename: tasks.kiru</span>

```kiru
use std;
use std::process;

rec Task {
  name: txt,
  line: txt,
};

fn run(task: Task) -> int {
  std::print("== @(task.name)");
  let code: int = std::process::command(task.line).run();
  match code {
    0 => { std::print("@(task.name): ok"); };
    _ => { std::print("@(task.name): failed (@(code))"); };
  };
  return code;
};

fn main() -> int {
  let task: Task = { name = "build", line = "echo building" };
  return run(task);
};
```

Three things are new here:

- `rec Task { … }` declares a type with two `txt` fields. A value of that type
  is written `{ name = "build", line = "cargo build" }`, and the annotation
  `let task: Task = …` tells the compiler which declared type the literal is.
  [Declared Types](/concepts/declared-types/) covers `rec` and `enum`.
- `run` takes a `Task` and returns an `int`. A function writes each parameter's
  type after its name and its return type after `->`. [Functions](/concepts/functions/)
  covers functions.
- `match code` chooses an arm by the exit code. `0` is the literal pattern for
  success, and `_` runs when no earlier arm matched. [Match](/concepts/match/)
  covers patterns and arms.

`std::print` writes a line to stdout. Its argument interpolates values with
`@(…)`: `@(task.name)` inserts the name and `@(code)` inserts the exit code as
decimal text. [Text](/concepts/text/) covers interpolation.

Run it:

```console
$ ./tasks
== build
building
build: ok
```

## Selecting a Task with std::cli

`main` receives the command-line words, but it does not parse flags; that is
`std::cli`'s job. A program declares a command tree and hands it the words:

<span class="filename">Filename: tasks.kiru</span>

```kiru
use std;
use std::cli;
use std::process;
use std::dynamic::Vec;

rec Task {
  name: txt,
  line: txt,
};

fn run(task: Task) -> int {
  std::print("== @(task.name)");
  let code: int = std::process::command(task.line).run();
  match code {
    0 => { std::print("@(task.name): ok"); };
    _ => { std::print("@(task.name): failed (@(code))"); };
  };
  return code;
};

fn build(context: std::cli::Context) -> int {
  let task: Task = { name = "build", line = "echo building" };
  return run(task);
};

fn test(context: std::cli::Context) -> int {
  let task: Task = { name = "test", line = "echo testing" };
  return run(task);
};

fn usage(context: std::cli::Context) -> int {
  std::print("tasks: run a project task");
  return 0;
};

fn main(words: Vec<txt>) -> int {
  let tasks: std::cli::Subcommand = std::cli::command("tasks")
    .about("run a project task")
    .action(usage)
    .add_child(std::cli::command("build").about("build the project").action(build))
    .add_child(std::cli::command("test").about("run the tests").action(test));
  return tasks.parse(words);
};
```

`std::cli::command(name)` starts a command named `name`, `.about(text)` sets its
description, `.add_child(child)` links a child command, and `.action(handler)` sets
the function that runs when the command is selected. Each method returns the
command, so the calls chain. `parse(words)` descends the tree by the leading
words and calls the selected command's action. An action takes a
`std::cli::Context` — the parsed command line — and returns an `int`, which
`main` returns as the exit status. A command that declares no flags ignores its
`Context`; [std::cli](/stdlib/cli/) covers flags and the rest of the tree.

`build` and `test` are ordinary functions passed to `.action` as values. A
function value must not be generic, so both are concrete.

Run it:

```console
$ ./tasks build
== build
building
build: ok
$ echo $?
0
$ ./tasks --help
tasks: run a project task
  build  build the project
  test  run the tests
```

## Capturing Output

A task runner often wants a command's output as data instead of printing it
live. `.capture()` runs the command and returns a `std::process::Output`, a
declared type with the fields `code`, `out`, and `err`:

<span class="filename">Filename: tasks.kiru</span>

```kiru
use std;
use std::process;

fn main() -> int {
  let result: std::process::Output = std::process::command("echo tests").capture();
  match result.code {
    0 => { std::print("tests passed"); };
    _ => {
      std::print("tests failed");
      std::print(result.err);
    };
  };
  return result.code;
};
```

`result.code` is an `int`, so matching it against `0` compares integers, and
`result.err` is the captured stderr as `txt`. `.run()` streams a command's
output live and returns only the code; `.capture()` captures both streams and
returns them. [Running Commands](/commands/running-a-command/) covers the
choice. `std::process::Output` is a declared `rec`, so `result.code` and
`result.err` are read as declared fields. [Declared
Types](/concepts/declared-types/) covers `rec`.

Run it:

```console
$ ./tasks
tests passed
```

## Where to Go Next

That is a complete Kiru program: it reads the command line, selects a task, runs
its command, and reports the result. The rest of the book explains each piece in
turn.

- [Common Concepts](/concepts/text/) covers text, maps, functions, and loops.
- [Namespaces](/concepts/modules/) covers splitting a program across
  directories.
- [Commands](/commands/running-a-command/) goes deeper into running processes.
- [Failure and Signals](/failure/panic/) covers what happens when a program
  fails.
