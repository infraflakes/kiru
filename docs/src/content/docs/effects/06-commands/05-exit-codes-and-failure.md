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

`std::eprint` writes to stderr and panics; `panic;` alone fails the run with
no output. Only `panic;` and a runtime failure fail a run.
