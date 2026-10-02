---
title: Input and output
description: stdout, stderr, and process groups.
---

Every command runs in its own process group and, on Linux, receives a
parent-death signal, so a stop reaches the whole tree.

`std::run` streams stdout live and captures it, and `std::command` selects
what to return through `Mode`. stderr is always forwarded, so errors remain
visible whatever the mode does with stdout.

    std::command({ Mode = "stdout" }, "echo shown");

An empty `Mode` discards stdout. A failed spawn and too many concurrent
commands are runtime errors.
