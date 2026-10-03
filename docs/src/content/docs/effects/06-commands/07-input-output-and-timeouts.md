---
title: Input and output
description: Streams and process groups.
---

Every command runs in its own process group and, on Linux, receives a
parent-death signal, so a stop reaches the whole tree.

`std::run` and `std::command` render a command's streams and return its exit
code; nothing is captured. `std::command`'s `Stream` entry selects the level:
empty renders stdout and stderr live, `"stderr"` hides stdout, and `"null"`
hides both. stderr stays visible in `"stderr"` mode, so errors are readable.

```kiru
std::command({ Stream = "null" }, "make");
```

A failed spawn and too many concurrent commands are runtime errors.
