---
title: The command spec
description: The uppercase entries of the std::command record.
---

`std::command(<spec>, <line>)` takes a flat record whose entries enable
features and a command line. Every entry is uppercase, and an absent or empty
entry is the default.

| entry | effect |
| --- | --- |
| `Mode` | `"stdout"` returns `out`, `"exit code"` returns `code`, empty runs quietly and returns `""` |
| `Dir` | runs after `cd <Dir> &&` |
| `Env` | prepended as `export <Env>;` |
| `Direnv` | wraps the line with `direnv exec <Dir> sh -c` |

    txt version = std::command({ Mode = "stdout" }, "git describe --always");
    txt code = std::command({ Dir = "docs", Mode = "exit code" }, "bun run build");

The line is shell syntax. A value that must not become shell syntax belongs in
`Env`, quoted by the caller.
