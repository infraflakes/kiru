---
title: Calls
description: Expression statements, command statements, and discarded results.
---

A statement can be a call or a command chain:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::print("hello");
  std::command("cargo test").stream();
};
```

The result of a call statement is discarded. A command chain that stands
alone as a statement runs the command and binds nothing.

## A Bare Statement Must Be a Call

A bare expression statement must be a call or a method chain. A call, a
native, a chain, an async spawn, and a wait are all legal:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn f() {
  return("");
};

fn work() {
  return("");
};

fn main() {
  f();
  std::command("x").out();
  std::async(work());
  std::wait();
  std::panic();
};
```

A literal, a variable, a record literal, or a concatenation as a statement
is a compile error:

```console
$ kc main.kiru
main.kiru:2:3: error: a statement must be a call or a method chain
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

## A Bare Command Runs

A command chain in a function body runs even without `.out` or `.code`.
It binds no value, and its stdout is not shown unless `.stream` is in the
chain:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("rm -rf build");   # runs; output is not shown
};
```

`.stream` shows stdout live. `.code` and `.out` bind text, and a chain
used as a statement discards that text after running:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("echo hi").stream();   # runs and shows "hi"
  std::command("true").code();        # runs; the code is discarded
};
```

```console
$ ./app
hi
```

## Call Statements and Functions

A call to a function returning text discards the text:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn compute() {
  return("ignored");
};

fn main() {
  compute();                    # returns "ignored", discarded
};
```

Nothing warns about discarded values. A program that observes a value
stores it, prints it, or returns it.

## Evaluation Order

In a call, arguments are evaluated left to right before the call. In a
chain, the target is evaluated before the method's arguments:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt version = "1";
rec env = { A = "1" };

fn main() {
  std::command("echo " + version).env(env).stream();
};
```

`version` and `env` are resolved before the command runs; the chain runs
once, when the statement executes, and the program prints `1`.
