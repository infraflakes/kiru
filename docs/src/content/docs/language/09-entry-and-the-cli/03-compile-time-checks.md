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
| Kinds | text where a record is required, a nothing call bound or passed |
| Returns | a value function that can fall through, a return of a nothing call |
| Cases | duplicate case data |
| Threads | an `async` nested inside another `async` |

Each failure prints a positioned diagnostic and exits `1`.

## Runtime Failures

The engine reports these failures at run time:

```text
spawn failure (command not found, permission denied)
too many concurrent commands
panic
```

Each records the failure, runs pending defers, and the process exits
nonzero once every thread joins. Signals also stop the running process
groups and exit `130`. [Exit codes and
failure](/effects/06-commands/05-exit-codes-and-failure/) covers the
failure model.

## Compilation Evaluates Nothing

Compilation reads sources and writes a binary. It runs no commands and no
program code. A program's top-level values are evaluated by the compiled
binary at startup, on the machine that runs it, which is why the exit code
of a startup command such as `std::command({}, "echo $HOME")` is not known
until the binary runs; [module
values](/language/03-names-and-scope/05-module-values/) covers that
evaluation.
