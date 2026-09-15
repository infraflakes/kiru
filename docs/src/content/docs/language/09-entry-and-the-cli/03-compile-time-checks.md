---
title: Compile-Time Checks
description: Everything the compiler proves, and the failures left for run time.
---

A program that compiles has been checked for:

| Check | Example failure |
| --- | --- |
| Syntax and lexical rules | invalid escape, missing `;` |
| Modules and imports | `module` not first, missing file, import cycle |
| Names | unknown name, forward reference, duplicate declaration, self-reference |
| Arity | wrong number of arguments |
| Parameters | a parameter without `txt` or `rec`, `main` with a `txt` parameter |
| Statements | a lone value or expression as a statement |
| Kinds | text where a record is required, a nothing call bound or passed |
| Returns | a value function that can fall through, a value return in a nothing function, a bare return in a value function, a return inside `defer` |
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

Each records the failure, runs pending defers, and the process exits nonzero
once every thread joins. [Exit codes and
failure](/effects/06-commands/04-exit-codes-and-failure/) covers the failure
model, and [Signals](/effects/08-failure-and-cleanup/02-signals/) covers exit
code 130.

## Compilation Evaluates Nothing

Compilation reads sources and writes a binary; it runs no commands and no
program code. [Module
values](/language/03-names-and-scope/04-module-values/) covers when top-level
values are evaluated.
