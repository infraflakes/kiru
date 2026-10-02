---
title: Streaming and capturing
description: How stdout and stderr are handled.
---

`std::run` streams stdout live and captures it at once, so a command's output
shows as it is produced and is also returned in `out`. stderr is always
forwarded.

`std::command` with `Mode = "stdout"` or `Mode = "exit code"` streams. An
empty `Mode` runs quietly and returns nothing:

| call | stdout |
| --- | --- |
| `std::command({ Mode = "stdout" }, line)` | shown and returned |
| `std::command({ Mode = "exit code" }, line)` | shown |
| `std::command({}, line)` | discarded |
