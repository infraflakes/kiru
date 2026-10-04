---
title: Running a command
description: std::run and the exit code.
---

`std::run(<line>)` runs one command line through the POSIX shell
(`/bin/sh -c`) and returns the exit code as text:

```kiru
txt code = std::run("make");
switch(code) {
  case("0") {};
  default { std::print("make failed"); };
};
```

A command's streams are inherited from the program, never captured;
[streaming and output](/effects/06-commands/03-terminals/) covers how a
command renders them.

`std::run` is the primitive. `std::command` is the record-driven abstraction
over it, and most code calls `std::command` instead.
