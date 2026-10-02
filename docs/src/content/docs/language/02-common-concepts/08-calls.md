---
title: Calls
description: Expression statements, command calls, and discarded results.
---

A statement can be a call:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::print("hello");
  std::command({ Mode = "stdout" }, "cargo test");
};
```

The result of a call statement is discarded. A `std::command` call that
stands alone as a statement runs the command and binds nothing.

## A Bare Statement Must Be a Call

A bare expression statement must be a call. A function call, a native call,
an async spawn, and a wait are all legal:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn work() {
  return "";
};

fn main() {
  work();
  std::command({ Mode = "stdout" }, "x");
  std::async(work());
  std::wait();
  panic;
};
```

A literal, a variable, a record literal, or a concatenation as a statement
is a compile error:

```console
$ kc main.kiru
main.kiru:2:3: error: a statement must be a call
  "x";
  ^^^
```

The same error covers `{ k = "v" };` and `a + b;`. Each computes a value
that nothing uses, so the language asks for a call that does something.

## A Void Call

A call to a void function has the `nothing` kind, so it may stand only as a
statement. Binding, passing, or returning it is a compile error; [return and
void](/language/02-common-concepts/07-return-and-recursion/) shows the
diagnostics.

## A Command Call Runs

A `std::command` call in a function body runs. The spec's `Mode` decides what
comes back and whether stdout shows:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command({}, "rm -rf build");                       # runs quietly
  std::command({ Mode = "stdout" }, "echo hi");           # runs and shows "hi"
  std::command({ Mode = "exit code" }, "true");           # runs; the code is discarded
};
```

```console
$ ./app
hi
```

`Mode = "stdout"` and `Mode = "exit code"` stream stdout live; an empty
`Mode` discards it. A call used as a statement discards the returned text
after running.

## Call Statements and Functions

A call to a function returning text discards the text:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn compute() {
  return "ignored";
};

fn main() {
  compute();                    # returns "ignored", discarded
};
```

Nothing warns about discarded values. A program that observes a value
stores it, prints it, or returns it.

## Evaluation Order

In a call, arguments are evaluated left to right before the call:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt version = "1";

fn main() {
  std::command({ Mode = "stdout" }, "echo " + version);
};
```

`version` is resolved before the command runs; the call runs once, when the
statement executes, and the program prints `1`.
