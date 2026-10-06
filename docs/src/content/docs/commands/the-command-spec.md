---
title: The Command Spec and Streams
description: The uppercase spec entries and how output is inherited or hidden.
---

A command's behavior is controlled by a small record called the spec. In this
section we'll list its entries and how each one changes a run.

`std::process::command(spec, line)` takes a flat record whose entries enable
features. Every entry is uppercase, and an absent or empty entry is the
default:

| Entry | Effect |
| --- | --- |
| `Dir` | run after `cd <Dir> &&`, with `Dir` quoted by `std::process::quote` |
| `Env` | prepend `export <Env>;`, used as written |
| `Timeout` | bound the line with `timeout <Timeout> sh -c` when not empty |
| `Stdin`/`Stdout`/`Stderr` | empty inherits the stream; `"null"` redirects it to `/dev/null` |

```kiru
std::process::command({ Dir = "docs" }, "bun run build");
std::process::command({ Stdout = "null", Stderr = "null" }, "command -v docker");
```

The line is shell syntax. A value that must not become shell syntax belongs in
`Env`, quoted by the caller, or is quoted by `std::process::quote`.

## Streams

A command's stdout and stderr are inherited and print live by default. Hiding
one keeps the other:

| Entry | empty | `"null"` |
| --- | --- | --- |
| `Stdout` | shown live | hidden |
| `Stderr` | shown live | hidden |

Hiding stdout while keeping stderr is the usual choice for a parallel step: its
stdout is noise, but a failure still prints its diagnostics. When a step needs
a value, `std::process::capture` returns both streams as `{ code, out, err }`.

`std::process::spawn` takes `Dir` and `Stdin`/`Stdout`/`Stderr` too.

A spawn that fails, and exceeding the concurrent-command limit, are runtime
errors. [Panic and Failure](/failure/panic/) covers runtime errors.
