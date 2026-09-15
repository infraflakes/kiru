---
title: Runtime Builtins
description: run and quote, the two builtins provided by the runtime.
---

The runtime provides exactly two names directly:

```text
std::run(text) -> text   run one command line and return its exit code
std::quote(text) -> text       escape text as one shell word
```

## std::run

`std::run` takes one command line, runs it through the POSIX shell
(`/bin/sh -c`), and returns the exit code as text. stdout and stderr are
inherited and rendered live; nothing is captured:

```kiru
fn main() {
  txt code = std::run("make");
  switch(code) {
    case("0") {};
    default { std::eprint("make failed"); };
  };
};
```

Quoting, pipes, and redirection are the shell's business. The record-driven
`std::command` is the usual way to run a command;
[the command spec](/effects/06-commands/02-builders/) describes it and its
`Stream` entry. [Running a
command](/effects/06-commands/01-building-a-command/) covers `std::run`.

## std::quote

`std::quote` wraps text in single quotes and escapes every embedded quote as
`'\''`, so the shell reads exactly the text:

```kiru
fn main() {
  txt line = "echo " + std::quote("it's here");
  std::run(line);
};
```

Use `std::quote` for text that must reach a command as data rather than as
shell syntax.
