---
title: Appendix A. Standard Library
description: Every shipped name, its signature, and its behavior.
---

## Runtime Builtins

```text
std::run(text) -> record { out, code }   run one command line
std::async(call) -> nothing               run a call on a new thread
std::wait() -> nothing                    join the calling thread's asyncs
```

[Runtime builtins](/stdlib/01-runtime-builtins/) describes each one.

## Keywords

```text
panic;          end the run, run defers, exit nonzero
return;         end a void function early
return expr;    end a function with text or record
```

## Written by Kiru

```text
std::command(spec, text) -> text    run one line and return the mode's text
std::print(text) -> nothing         write a line to stdout
std::eprint(text) -> nothing        write a line to stderr, then panic; "" silently
```

[Written by Kiru](/stdlib/02-written-by-kiru/) shows the source and the spec.

## The Command Spec

`std::command` takes a flat record whose uppercase entries enable features.
[The command spec](/effects/06-commands/02-builders/) documents the behavior.

| Entry | Effect |
| --- | --- |
| `Mode` | `"stdout"`, `"exit code"`, or empty |
| `Dir` | run after `cd <Dir> &&` |
| `Env` | prepend `export <Env>;` |
| `Direnv` | wrap with `direnv exec <Dir> sh -c` |
