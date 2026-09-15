---
title: Types
description: Text and record are data; nothing is the result of a call that returns no value.
---

Every value has one of three kinds:

```text
text       string data
record     a map of names to text
nothing    the result of a call that returns no value
```

`text` and `record` are data. `nothing` is an execution marker: it describes the no-value result of a call that returns no value, not data the program stores. [Return and nothing](/language/02-common-concepts/07-return-and-recursion/) covers `nothing` and how it arises.

`text` is the only scalar. Numbers, versions, paths, and flags are all text. `record` is a map of names to text; it does not nest.

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt name = "backend";                              # text
rec env = { RUST_BACKTRACE = "1" };                # record
rec backend = { name = "backend", dir = "." };     # a record with keys
```
