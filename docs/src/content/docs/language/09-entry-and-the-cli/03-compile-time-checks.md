---
title: Compile-Time Checks
description: Everything the compiler proves, and the failures left for run time.
---

The compiler is where correctness is decided. A program that compiles has
been checked for:

| Check | Example failure |
| --- | --- |
| Syntax and lexical rules | invalid escape, missing `;` |
| Modules and imports | `module` not first, missing file, import cycle |
| Names | unknown name, forward reference, duplicate declaration, self-reference |
| Arity | wrong number of arguments |
| Parameters | a parameter without `txt` or `rec`, `main` with a `txt` parameter |
| Statements | a lone value or expression as a statement |
| Kinds | text where a record is required, command bound to a `txt`, a void call bound or passed |
| Returns | a return that is not last, a return in `main`, a return of a void call |
| Cases | duplicate case data |
| Chain rules | a method twice, `.out()` and `.code()` together |
| Threads | `std::async` outside a function |

Each failure prints a positioned diagnostic and exits `1`.

## Runtime Failures

The engine reports these failures at run time:

```text
empty dir
invalid computed timeout
spawn failure (command not found, permission denied)
direnv failure
too many concurrent commands
std::panic
```

Each records the failure, runs pending defers, and the process exits
nonzero once every thread joins. Signals also stop the running process
groups and exit `130`. [Exit codes and
failure](/effects/06-commands/05-exit-codes-and-failure/) covers the
failure model.

## Compilation Evaluates Nothing

Compilation reads sources and writes a binary. It runs no commands and no
program code. A program's top-level values are evaluated by the compiled
binary at startup, on the machine that runs it, which is why a home
directory comes from `std::command("echo $HOME").out()`; [module
values](/language/03-names-and-scope/05-module-values/) covers that
evaluation.
