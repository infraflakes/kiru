---
title: Modes
description: stdout, exit code, and the empty mode.
---

`Mode` decides what `std::command` returns:

| Mode | returns |
| --- | --- |
| `"stdout"` | the captured stdout |
| `"exit code"` | the exit code as text |
| empty | `""`, and the line runs quietly |

    txt out = std::command({ Mode = "stdout" }, "echo hi");
    txt code = std::command({ Mode = "exit code" }, "exit 3");
    std::command({}, "touch built");

A function has one kind, so `std::command` always returns text; the empty mode
returns the empty string, which the call site discards.
