---
title: Appendix A. Standard Library
description: Every shipped name, its signature, and its behavior.
---

## Runtime Builtins

```text
std::run(text) -> text   run one command line and return its exit code
std::quote(text) -> text       escape text as one shell word
```

[Runtime builtins](/stdlib/01-runtime-builtins/) describes each one.

## Keywords

```text
panic;          end the run, run defers, exit nonzero
return();       end a function with no return kind early
return(expr);   end a `-> txt`/`-> rec` function early
async <call>;   run a call on a new thread
wait;           join the calling thread's asyncs
```

## Written by Kiru

```text
std::command(spec, line) -> text    run one line and return its exit code
std::print(text) -> nothing         write a line to stdout
std::log(text) -> nothing           write an "INFO:" line to stdout
std::eprint(text) -> nothing        write an "ERROR:" line to stderr, then panic
```

`std::eprint` ends in `panic;`, so a call to it stops the run and can end a value function's path. [Written by Kiru](/stdlib/02-written-by-kiru/) shows the source and the spec.

## The Command Spec

`std::command` takes a flat record whose uppercase entries enable features.
[The command spec](/effects/06-commands/02-builders/) documents the behavior.

| Entry | Effect |
| --- | --- |
| `Dir` | run after `cd <Dir> &&`, with `Dir` quoted by `std::quote` |
| `Env` | prepend `export <Env>;`, used as written |
| `Nix` | run inside `nix develop -c sh -c` only when it is exactly `"true"`, quoting the line |
| `Direnv` | wrap with `direnv exec <Dir> sh -c` only when it is exactly `"true"`, quoting `Dir` and the line |
| `Timeout` | bound with `timeout <Timeout> sh -c` when `Timeout` is not empty, using it as written and quoting the line |
| `Stream` | wrap the line so stdout and/or stderr are hidden: empty renders both, `"stderr"` hides stdout, `"null"` hides both |
