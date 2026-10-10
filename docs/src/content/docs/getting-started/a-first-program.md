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
fn main(args<rec>) {
  std::print("command: " + args.cmd);
};
```

When `main` declares `args<rec>`, the runtime builds a record from the words
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
instead. Use `match` to compare `args.cmd` against the commands we know:

<span class="filename">Filename: tasks.kiru</span>

```kiru
fn main(args<rec>) {
  match args.cmd {
    "test" => { std::print("running tests"); };
    "build" => { std::print("building"); };
    _ => { std::eprint("unknown command: " + args.cmd); };
  };
};
```

`match` compares a text against patterns and runs the first arm that matches. A
pattern can be a string literal, a name, or a field path, and the `_` arm runs
when nothing matched.

`std::print` writes a line to stdout. `std::eprint` writes an `ERROR:`
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
`std::command` runs a command line through the shell and returns its
exit code as text:

<span class="filename">Filename: tasks.kiru</span>

```kiru
fn run(line<txt>) -> txt {
  return std::command({}, line);
};

fn main(args<rec>) {
  match args.cmd {
    "test" => { run("cargo test"); };
    "build" => { run("cargo build"); };
    _ => { std::eprint("unknown command: " + args.cmd); };
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
fn run(line<txt>) -> txt {
  return std::command({}, line);
};

fn report(name<txt>, code<txt>) {
  match code {
    "0" => { std::print(name + " passed"); };
    _ => { std::print(name + " failed with code " + code); };
  };
};

fn main(args<rec>) {
  match args.cmd {
    "test" => { report("tests", run("cargo test")); };
    "build" => { report("build", run("cargo build")); };
    _ => { std::eprint("unknown command: " + args.cmd); };
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
`std::capture` runs a command line and returns a record with the exit
code and both streams:

<span class="filename">Filename: tasks.kiru</span>

```kiru
fn main(args<rec>) {
  let result<rec> = std::capture("cargo test");
  match result.code {
    "0" => { std::print("tests passed"); };
    _ => {
      std::print("tests failed");
      std::print(result.err);
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
