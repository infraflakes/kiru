---
title: A First Program
description: Build a small task runner and meet the basics of Kiru.
---

Let's jump in by building something useful. In this chapter we'll write a small
task runner: a program that runs a command for you and reports whether it
succeeded. Along the way you'll meet most of the pieces a real Kiru program
uses, and the chapters after this one explain each piece in detail.

Here is what the program will do:

- read the command from the command line;
- run the matching command;
- report whether it passed or failed.

## Reading the Command Line

Make a new file called `tasks.kiru` and start with a `main` that prints what it
received:

<span class="filename">Filename: tasks.kiru</span>

```kiru
fn main(rec args) {
  std::io::print("command: " + args.cmd);
};
```

When `main` declares `rec args`, the runtime builds a record from the words
after the program name and passes it in. The record has two fields: `cmd`, the
first word, and `flag`, the second word.

Compile and run it:

```console
$ kc tasks.kiru
$ ./tasks test
command: test
$ ./tasks
command:
```

A missing word reads as `""`, so a program never has to guard a lookup.

## Branching on the Command

Right now the program only prints the command. Let's make it do something
instead. Use `switch` to compare `args.cmd` against the commands we know:

<span class="filename">Filename: tasks.kiru</span>

```kiru
fn main(rec args) {
  switch(args.cmd) {
    case("test") { std::io::print("running tests"); };
    case("build") { std::io::print("building"); };
    default { std::io::eprint("unknown command: " + args.cmd); };
  };
};
```

`switch` compares a text against case patterns and runs the first arm that
matches. A pattern can be a string literal, a name, or a field path, and
`default` runs when nothing matched.

`std::io::print` writes a line to stdout. `std::io::eprint` writes an `ERROR:`
line to stderr and then exits the program, which is what we want for an unknown
command:

```console
$ ./tasks test
running tests
$ ./tasks nope
ERROR: unknown command: nope
$ echo $?
1
```

## Running a Command

Printing a message is not much of a task runner. Let's actually run the command.
`std::process::command` runs a command line through the shell and returns its
exit code as text:

<span class="filename">Filename: tasks.kiru</span>

```kiru
fn run(txt line) -> txt {
  return(std::process::command({}, line));
};

fn main(rec args) {
  switch(args.cmd) {
    case("test") { run("cargo test"); };
    case("build") { run("cargo build"); };
    default { std::io::eprint("unknown command: " + args.cmd); };
  };
};
```

We added a function, `run`. A function is declared with `fn`, lists its
parameters with their types, and declares its return type after `->`. Here
`run` takes one `txt` and returns a `txt`: the exit code.

Notice that `run` is written above `main`. A name must be declared before the
code that uses it, so helper functions go before the functions that call them.
[Names and Scope](/concepts/names-and-scope/) explains the rule.

A command's exit code is ordinary text, and `"0"` means success. Run the
program:

```console
$ ./tasks build
   Compiling tasks v0.1.0
    Finished dev [unoptimized + debuginfo] target(s) in 0.42s
$ echo $?
0
```

The command's output prints live, because a command inherits the program's
stdout and stderr by default.

## Reporting the Result

The exit code is data, and nothing fails when it is nonzero. Let's check it and
report the result ourselves:

<span class="filename">Filename: tasks.kiru</span>

```kiru
fn run(txt line) -> txt {
  return(std::process::command({}, line));
};

fn report(txt name, txt code) {
  switch(code) {
    case("0") { std::io::print(name + " passed"); };
    default { std::io::print(name + " failed with code " + code); };
  };
};

fn main(rec args) {
  switch(args.cmd) {
    case("test") { report("tests", run("cargo test")); };
    case("build") { report("build", run("cargo build")); };
    default { std::io::eprint("unknown command: " + args.cmd); };
  };
};
```

Now the program tells us what happened:

```console
$ ./tasks test
... test output ...
tests passed
```

## Capturing Output

Sometimes you want a command's output as a value instead of printing it live.
`std::process::capture` runs a command line and returns a record with the exit
code and both streams:

<span class="filename">Filename: tasks.kiru</span>

```kiru
fn main(rec args) {
  rec result = std::process::capture("cargo test");
  switch(result.code) {
    case("0") { std::io::print("tests passed"); };
    default {
      std::io::print("tests failed");
      std::io::print(result.err);
    };
  };
};
```

The record has three fields: `code`, `out`, and `err`. A record is a map of
names to text, and reading a missing field gives `""`.

## Where to Go Next

That is a complete Kiru program: it reads the command line, branches, runs
commands, and reports the result. The rest of the book explains each piece in
turn.

- [Common Concepts](/concepts/text/) covers text, records, functions, and loops.
- [Namespaces and Imports](/concepts/namespaces/) covers splitting a program
  across files.
- [Commands](/commands/running-a-command/) goes deeper into running processes.
- [Failure and Signals](/failure/panic/) covers what happens when a program
  fails.
