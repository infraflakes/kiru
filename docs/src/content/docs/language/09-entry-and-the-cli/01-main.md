---
title: The Entry Function
description: The entry function, its signature, and dispatch.
---

Running a compiled program calls the entry file's own `main`, declared in the
root namespace:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::print("no arguments");
};
```

## Signature

- The entry file must declare `main` in the root namespace.
- `main` takes zero or one parameter.
- The one-parameter form receives the args record; [Command Line
  Arguments](/language/09-entry-and-the-cli/02-the-args-record/) describes it.
- `main` is an ordinary function: its kind is derived like any other, and the
  runtime discards whatever it returns. It may `return();` or `return(expr);`.
- A `main` declared inside a module is an ordinary function.

An entry file with no `main`, or a `main` with two parameters, is a compile
error.

[The entry file](/language/05-modules-and-namespaces/03-the-entry-file/)
covers the root-namespace and duplicate-`main` rules.

`main` is a dispatcher; the usual shape is a nested `switch`, as
[Switch](/language/02-common-concepts/10-switch/) shows.

`main`'s value is discarded, so a program's result is its exit code;
[Panic](/effects/08-failure-and-cleanup/01-panic/) covers failure, and
[Signals](/effects/08-failure-and-cleanup/02-signals/) covers exit code 130.

There is no declaration for arguments and no generated help: the program
decides what arguments mean and what usage to print.
