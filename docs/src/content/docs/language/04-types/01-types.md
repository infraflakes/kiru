---
title: Types
description: Text and record are data; nothing is the result of a void call.
---

Every value has one of three kinds:

```text
text       string data
record     a map of names to text
nothing    the result of a void call
```

`text` and `record` are data. `nothing` is an execution marker: it describes
the no-value result of a void call, not data the program stores.

## Data

`text` is the only scalar. Numbers, versions, paths, and flags are all text.
`record` is a map of names to text; it does not nest.

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt name = "backend";                              # text
rec env = { RUST_BACKTRACE = "1" };                # record
rec backend = { name = "backend", dir = "." };     # a record with keys
```

The first declaration binds text, and the next two bind records. The
`backend` record carries the keys `name` and `dir`; nothing about it is
special to the type system.

## Nothing

`nothing` is the result of a void call. A function without a value return is
void, and its call stands only as a discarded statement:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn note(txt message) {
  std::print(message);
};

fn main() {
  note("hello");
};
```

`panic;` is a statement, not a value, so it cannot stand as an argument, a
binding, or a returned value.

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn stop() {
  panic;
};
```

`stop` is void: it has no `return`, and `panic;` is its last statement.
