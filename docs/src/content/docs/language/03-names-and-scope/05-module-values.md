---
title: Module Values
description: Top-level values, when they are evaluated, and what may initialize them.
---

A `txt` or `rec` declared at the top level of a file is a *module
value*. Module values are evaluated once, at program startup, in
declaration order, before `main` runs:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt home = std::command("echo $HOME").out();

rec backend = {
  name = "backend",
  dir = home + "/projects/backend",
};
```

The command runs on the machine that runs the program, once, at startup.
`backend.dir` is then built from the captured text, which is how a path
under a home directory is assembled.

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
initializer. A command chain is allowed:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt version = std::command("git describe --always").out();
```

A thread cannot start at module level: `std::async` is only allowed
inside a function body. The declaration order is the evaluation order, so a
value can reference functions and values declared above it, but not below
it: names are read top-down everywhere, module values included.

## At Startup

Compiling the program runs no commands. The compiled binary evaluates the
module values when it starts, so `version` above is the output of `git
describe` on the machine that runs the binary, not on the machine that
compiled it. A home directory needs no special syntax because
`std::command("echo $HOME").out()` asks the machine directly.

## Module Values and Bodies

A function body works the same way, with one addition: a command chain may
stand bare as a statement, where it runs and binds nothing:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("cargo test").stream();
};
```

[Statement calls](/language/02-common-concepts/08-calls/) covers that form.
