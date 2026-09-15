---
title: The command spec
description: The uppercase entries of the std::command record.
---

`std::command(<spec>, <line>)` takes a flat record whose entries enable
features and a command line, and returns the exit code. Every entry is
uppercase, and an absent or empty entry is the default.

| entry | effect |
| --- | --- |
| `Dir` | runs after `cd <Dir> &&`, with `Dir` quoted by `std::quote` |
| `Env` | prepended as `export <Env>;`, used as written |
| `Nix` | runs the line inside `nix develop -c sh -c` only when it is exactly `"true"`, quoting the line |
| `Direnv` | wraps the line with `direnv exec <Dir> sh -c` only when it is exactly `"true"`, quoting `Dir` and the line |
| `Timeout` | bounds the line with `timeout <Timeout> sh -c` when `Timeout` is not empty, using it as written and quoting the line |
| `Stream` | hides stdout and/or stderr; [stream levels](/effects/06-commands/03-terminals/) |

```kiru
txt code = std::command({ Dir = "docs" }, "bun run build");
std::command({ Stream = "null" }, "command -v docker");
```

The line is shell syntax. A value that must not become shell syntax belongs in
`Env`, quoted by the caller, or is quoted by `std::quote`.
