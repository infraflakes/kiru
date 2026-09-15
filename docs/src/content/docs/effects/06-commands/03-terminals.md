---
title: Streaming and Output
description: How a command's stdout and stderr are rendered.
---

A command's stdout and stderr are rendered, never captured. stdout is the
progress and result a person reads; stderr is the diagnostics. Both are
inherited from the program by default, so a tool that detects a terminal
behaves as it would in a shell. `std::command` selects the level through the
`Stream` entry, which wraps the line so hidden streams go to `/dev/null`:

| `Stream` | stdout | stderr |
| --- | --- | --- |
| empty | shown live | shown live |
| `"stderr"` | hidden | shown live |
| `"null"` | hidden | hidden |

```kiru
std::command({}, "echo hi");                 # both streams show
std::command({ Stream = "stderr" }, "make"); # only stderr shows
std::command({ Stream = "null" }, "make");   # nothing shows
```

`"stderr"` is the usual choice for a parallel step: its stdout is noise, but a
failure still prints its diagnostics.

Because nothing is captured, a step that needs a value writes it to a file
for the next command to read; the program itself only sees exit codes.

Terminal ownership is per thread. A command run off the entry thread renders
nothing, whatever `Stream` says; [threads](/effects/07-threads/01-threads/)
covers that rule.

A spawn that fails and too many concurrent commands are runtime errors.
