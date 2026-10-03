---
title: Streaming levels
description: What a command renders while it runs.
---

`std::command` renders a command's streams through the `Stream` entry, which
wraps the line so the hidden streams go to `/dev/null`. Nothing is ever
captured, so the choice is only about what a person sees:

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
