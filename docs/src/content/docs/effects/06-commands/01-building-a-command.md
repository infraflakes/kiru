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
  default { std::eprint("make failed"); };
};
```

The command's output is never captured. stdout and stderr are inherited, so
output appears as it is produced and a tool that detects a terminal behaves as
it would in a shell. `std::command`'s `Stream` entry can hide stdout and/or
stderr when a step is noisy.

`std::run` is the primitive. `std::command` is the record-driven abstraction
over it, and most code calls `std::command` instead.
