---
title: Module Values
description: Top-level values, when they are evaluated, and what may initialize them.
---

A `txt` or `rec` declared at the top level of a file is a *module
value*. Module values are evaluated once, at program startup, in
declaration order, before `main` runs:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt home = "/home/dev";

rec backend = {
  name = "backend",
  dir = home + "/projects/backend",
};
```

`home` is evaluated once at startup, before `main` runs. `backend.dir` is
then built from it by concatenation, which is how a path under a home
directory is assembled.

## One Value per Name

A reference to a module value always reads the one stored value. The
initializer never runs again:

<span class="filename">Filename: src/main.kiru</span>

```kiru
rec backend = {
  name = "backend",
  dir = "/tmp/backend",
};

fn where() {
  return(backend.dir);
};
```

`where` reads the `backend` record the startup pass stored, no matter how
many times it is called.

## What May Initialize a Module Value

Any expression that produces text or a record may be a module value's
initializer. A call is allowed:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt cargo_present = std::command({}, "test -f Cargo.toml");
```

A thread cannot initialize a value, because `async` is a keyword statement
that spawns a call and binds nothing, while a value binding needs data; the
error is the generic kind mismatch. The declaration order is the evaluation
order, so a value can reference functions and values declared above it, but
not below it: names are read top-down everywhere, module values included.

## At Startup

Compiling the program runs no commands. The compiled binary evaluates the
module values when it starts, so `cargo_present` above is the exit code of
`test -f Cargo.toml` on the machine that runs the binary, not on the machine
that compiled it. A command that must inspect the machine it runs on is
simply run at startup, and its exit code is the value Kiru sees.

## Module Values and Bodies

A function body works the same way, with one addition: a call may stand bare
as a statement, where it runs and binds nothing:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command({}, "cargo test");
};
```

[Statement calls](/language/02-common-concepts/08-calls/) covers that form.
