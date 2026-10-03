---
title: Output and errors
description: Where a command's stdout and stderr go.
---

A command's stdout and stderr are rendered, never captured. stdout is the
progress and result a person reads; stderr is the diagnostics. Both are
inherited from the program by default, so a tool that detects a terminal
behaves as it would in a shell.

`std::command` selects the level through `Stream`:

| call | stdout | stderr |
| --- | --- | --- |
| `std::command({}, line)` | shown live | shown live |
| `std::command({ Stream = "stderr" }, line)` | hidden | shown live |
| `std::command({ Stream = "null" }, line)` | hidden | hidden |

Because nothing is captured, a step that needs a value writes it to a file and
lets the next command read it; the program itself only sees exit codes.

A command run on a thread started by `async` is always silent: that thread
does not own the terminal, so its stdout and stderr are discarded whatever
`Stream` says. [Threads](/effects/07-threads/01-threads/) covers the rule.
