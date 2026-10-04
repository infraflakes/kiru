---
title: Exit codes and failure
description: Exit codes are data; panic fails the run.
---

`std::run` and `std::command` return the exit code as text; `"0"` means
success. An unobserved code fails nothing:

```kiru
txt code = std::command({}, "false");
switch(code) {
  case("0") {};
  default { std::eprint("command failed"); };
};
```

A run fails only on `panic;`, on `std::eprint`, or on a runtime error the
engine detects. [Panic](/effects/08-failure-and-cleanup/01-panic/) covers
`std::eprint` and how a failing run unwinds.
