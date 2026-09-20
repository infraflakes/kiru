---
title: Builders
description: in, direnv, env, shell, and timeout.
---

Builders configure a chain before it runs. Each returns a new chain, and
calling the same builder twice in one chain is a compile error.

| Method | Accepts | Returns | Meaning |
| --- | --- | --- | --- |
| `.in(text)` | text | command | Sets the working directory. An empty path is a runtime error. |
| `.direnv()` | none | command | Approves the directory with `direnv allow` and wraps the command with `direnv exec`. A chain with `.direnv()` but no `.in()` fails at run time with `direnv needs a dir`. |
| `.env(record)` | record | command | Adds or overrides environment variables for this command. |
| `.shell(text)` | text | command | Runs the command as `<program> -c <line>`. When set it is used verbatim, an empty value included; only when unset does `$SHELL`, then `sh`, apply. |
| `.timeout(text)` | text seconds | command | Whole seconds with no unit suffix. `"0"` disables the timeout. An invalid literal is a compile error; an invalid computed value is a runtime error. On expiry the group receives SIGTERM, then SIGKILL after two seconds, and the code becomes `"124"`. |

## Entering a Directory

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("cargo test")
    .in(".")
    .direnv()
    .stream();
};
```

`.in` sets the working directory from plain text, like any other
argument. `.direnv` approves that directory with `direnv allow` and wraps
the command with `direnv exec`, so the directory's `.envrc` applies. The
program decides whether to call it. A chain with `.direnv` but no
`.in` fails at run time with `direnv needs a dir`, and a chain with an
empty directory fails at run time with the command named.

## Setting the Environment

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("cargo build")
    .env({ CARGO_TERM_COLOR = "never", RUST_BACKTRACE = "1" })
    .stream();
};
```

`.env` adds or overrides variables for this command only. It does
not affect other commands, and there are no global variables.

## Choosing the Shell

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("echo $0")
    .shell("/bin/bash")
    .stream();
};
```

When `.shell` is set, its value is used verbatim, empty included. An empty
shell fails at run time, because the runtime tries to run a program named by
an empty string. Only when `.shell` is unset does `$SHELL`, then `sh`,
apply. `.shell` is for the cases where the program must control the
interpreter, such as forcing POSIX behavior.

## Timeouts

The value is whole seconds as text. Here `sleep 5` is stopped after one
second, and the command's code is `"124"`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  txt code = std::command("sleep 5").timeout("1").code();
  std::print("code " + code);
};
```

```console
$ ./app
code 124
```

`"0"` disables the timeout. A literal timeout is validated at compile time:

```console
$ kc main.kiru
main.kiru:1:47: error: a timeout literal must be whole seconds
fn main() { txt x = std::command("x").timeout("1s").code(); };
                                              ^^^^
```

A computed value that is not whole seconds fails at run time with the
command named; [input, output, and
timeouts](/effects/06-commands/07-input-output-and-timeouts/) describes the
execution.
