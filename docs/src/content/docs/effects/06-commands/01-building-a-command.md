---
title: Building a Command
description: std::command, immutability, and the chain model.
---

`std::command(<text>)` builds a command chain from one command line:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("echo hello").stream();
};
```

The line is passed to a shell, so pipes, redirection, and globbing work as
they do in a terminal. The command runs when the chain is evaluated; here
the statement runs it and `.stream` shows its output.

## Chains Are Immutable

Every builder returns a new chain; it does not modify the one it is called
on. Calling the same method twice in one chain is a compile error:

```console
$ kc main.kiru
main.kiru:2:3: error: `.in` is called twice in one command chain
  std::command("ls").in("/a").in("/b").stream();
  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The directory and direnv are separate settings: `.in` chooses the
directory, and `.direnv` wraps the command with `direnv exec` in that
directory:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("cargo build")
    .in(".")
    .direnv()
    .env({ CARGO_TERM_COLOR = "never" })
    .timeout("600")
    .stream();
};
```

Each builder configures the same chain, and the statement runs it.
[Builders](/effects/06-commands/02-builders/) documents each setting.

## The Chain Runs Once

A chain runs when it is evaluated. `.stream` only marks that stdout
shows; it keeps the chain going. `.out` and `.code` are the only
terminals, and each runs the command and returns text:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  txt code = std::command("true").stream().code();
  std::print("code " + code);
};
```

```console
$ ./app
code 0
```

The command runs once, when the chain is evaluated, and `code` binds `0`.

## A Standalone Chain Runs

When a chain stands alone as a statement in a function body, it runs and
binds nothing; [statement calls](/language/02-common-concepts/08-calls/) covers
that form.

## A Function Runs a Chain

A function cannot return a chain; a command is not data. A function that
needs a configured command runs it and returns text:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn cargo(txt dir, txt sub) {
  return(std::command("cargo " + sub)
    .in(dir)
    .timeout("600")
    .stream()
    .code());
};

fn clippy(txt dir) {
  return(cargo(dir, "clippy -- -D warnings"));
};
```

`cargo` builds and runs the chain and returns the exit code as text; `clippy`
passes that text up. The command is consumed inside `cargo`, so no command
value has to be stored or returned.
