---
title: The Entry Function
description: The two signatures, the void rule, and dispatch.
---

Running a compiled program calls the entry file's own `main`, declared in
the root namespace:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::print("no arguments");
};
```

A program that reads the command line declares one parameter instead:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main(rec args) {
  std::print("command: " + args.cmd);
};
```

The parameter is optional. A program that does not read the command line
declares `fn main()`; a program that does declares `fn main(rec <name>)`,
and the parameter name is chosen by the author.

## Signature

- The entry file must declare `main` in the root namespace.
- `main` takes zero or one parameter.
- A parameter must be declared `rec` and receives the args record.
- `main` is void: no caller binds its value, so `return;` and `return expr;`
  both end it and a returned value is discarded.
- A `main` declared inside a module is an ordinary function.

An entry file with no `main`, or a `main` with two parameters, is a
compile error:

```console
$ kc main.kiru
main.kiru:1:4: error: `main` takes at most one parameter
fn main(rec a, rec b) {};
   ^^^^
```

[The entry file](/language/05-modules-and-namespaces/03-the-entry-file/)
covers the root-namespace and duplicate-`main` rules.

## Dispatch

`main` is a dispatcher. The usual shape is a `switch` on `args.cmd`, with a
nested `switch` on `args.flag`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn deploy_all() {
  std::print("deploying everything");
};

fn deploy(txt name) {
  std::print("deploying " + name);
};

fn main(rec args) {
  switch(args.cmd) {
    case("deploy") {
      switch(args.flag) {
        case("") { deploy_all(); };
        case("backend") { deploy("backend"); };
        default { std::eprint("usage: deploy [backend]"); };
      };
    };
    default { std::eprint("usage: deploy [backend]"); };
  };
};
```

`./app deploy` runs `deploy_all`; `./app deploy backend` runs `deploy`; any
other input prints the usage line to stderr and exits nonzero. The helpers
are void, so each call is a statement.

## The Result

`main` produces no value. A program's result is its exit code: `0` when the
body finishes, nonzero on panic, and `130` on SIGINT, SIGTERM, or SIGHUP.
To fail a run, run `panic;` or call `std::eprint`; [exit codes and
failure](/effects/06-commands/05-exit-codes-and-failure/) covers
failure.

## No Generated Help

There is no flag declaration and no help text generated from one. The
program decides what arguments mean and what usage to print. The command
line is `cmd` plus `flag`, as [the args
record](/language/09-entry-and-the-cli/02-the-args-record/) describes.
