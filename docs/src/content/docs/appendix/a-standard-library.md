---
title: Appendix A. Standard Library
description: Every shipped name, its signature, and its behavior.
---

## Runtime Builtins

```text
std::command(text) -> command     a chain from one command line
std::async(call) -> nothing       run a call on a new thread
std::wait() -> nothing            join the calling thread's asyncs
std::panic() -> never             record failure, run defers, exit nonzero
```

[Runtime builtins](/stdlib/01-runtime-builtins/) describes
each one.

## Written by Kiru

```text
std::print(text) -> text           write a line to stdout, returns ""
std::eprint(text) -> nothing       write a line to stderr, then panic; "" silently
```

[Written by
Kiru](/stdlib/02-written-by-kiru/) shows the
source and the environment-passing rule.

## Builders

Each method returns a new command chain, and calling one twice in a chain
is a compile error. [Builders](/effects/06-commands/02-builders/) documents the
behavior.

| Method | Accepts | Returns |
| --- | --- | --- |
| `.in(text)` | text | command |
| `.direnv()` | none | command |
| `.env(record)` | rec | command |
| `.shell(text)` | text | command |
| `.timeout(text)` | text seconds | command |
| `.stream()` | command | command |

## Terminals

Each terminal runs the chain and returns text; `.out` and `.code`
cannot be combined. [Terminals](/effects/06-commands/03-terminals/) documents
the rules.

| Terminal | Accepts | Returns |
| --- | --- | --- |
| `.out()` | command | text |
| `.code()` | command | text |
