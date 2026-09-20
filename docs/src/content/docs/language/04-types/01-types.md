---
title: Types
description: Text and record are data; command, nothing, and never are markers.
---

Every value has one of five kinds:

```text
text       string data
record     a map of names to text

command    a command chain that has not run yet
nothing    the result of a void call
never      a call that does not return
```

`text` and `record` are data. The other three are execution markers: they
describe values that exist for the engine, not data the program stores.

## Data

`text` is the only scalar. Numbers, versions, paths, and flags are all text.
`record` is a map of names to text; it does not nest and does not hold
command chains.

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt name = "backend";                              # text
rec env = { RUST_BACKTRACE = "1" };                # record
rec backend = { name = "backend", dir = "." };     # a record with keys
```

The first declaration binds text, and the next two bind records. The
`backend` record carries the keys `name` and `dir`; nothing about it is
special to the type system.

## Markers

A `command` is a command chain plus its context. It stands as a statement
or as the target of another builder, and `.out` or `.code` turns it into
text:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt version = std::command("git describe --always").out();
```

The chain runs when the declaration is evaluated, and `version` binds the
text it produced; [building a
command](/effects/06-commands/01-building-a-command/) covers the chain model.

`nothing` is the result of a void call. A function without `return` is
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

`never` is the result of `std::panic`. A call that never returns ends the
run, and it stands only as a statement:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn stop() {
  std::panic();
};
```

`stop` is void: it has no `return`, and `std::panic` is the last statement.
A `never` call cannot stand as an argument, a binding, or a returned value.
