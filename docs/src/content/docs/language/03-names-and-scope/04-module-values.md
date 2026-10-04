---
title: Module Values
description: Top-level values, when they are evaluated, and what may initialize them.
---

A `txt` or `rec` declared at the top level of a file is a *module value*. Module values are evaluated once, at program startup, in declaration order, before `main` runs:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt home = "/home/dev";

rec backend = {
  name = "backend",
  dir = home + "/projects/backend",
};
```

A reference to a module value always reads the one stored value; the initializer never runs again. The initializers run when the compiled program starts, not when it is compiled.

Any expression that produces text or a record may initialize a module value, and a call is allowed:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn default_dir() -> txt {
  return("/tmp/backend");
};

rec tool = {
  name = "tool",
  dir = default_dir(),
};
```

Declaration order is evaluation order, so a module value may reference functions and values declared above it, but not below it: names are read top-down everywhere, module values included. [Assignment and read-only parameters](/language/02-common-concepts/08-assignment/) covers why a module value cannot be assigned.
