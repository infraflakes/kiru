---
title: Maps
description: The Map type, empty and of, get and get_or and insert, and the ordered association.
---

A map is an **ordered association** from keys to values. It is keyed by
equality: writing a key that is already present replaces its value and keeps its
place, and a new key appends. A vec finds an element by its position; a map finds
a value by its key.

`Map` is not a built-in type. It is an ordinary declared type — a `rec` — in the
`std::map` namespace, built over a vec:

```kiru
use std::vec;
rec Entry<K, V> { pub key: K, pub value: V };
pub rec Map<K, V> { entries: Vec<Entry<K, V>> };
```

`use std::map;` brings `Map` and the map methods into scope. A qualified
`std::map::Map` names the type without it, but a method call like `m.get(...)`
still needs the `use`, the same as any other method
([Methods](/concepts/methods/) covers reachability). `Entry` and `Map`'s one
field, `entries`, are **private** to `std::map`: a program never names `Entry`,
never writes `{ entries = … }`, and never reads `m.entries`. A map is built and
read only through its methods. [std::map](/stdlib/map/) lists every signature.

## Building a map

A map starts empty and grows with `with`:

```kiru
use std;
use std::text;
use std::map;
fn main() {
  let env: Map<txt, txt> = std::map::empty()
    .with("RUST_BACKTRACE", "1")
    .with("CARGO_TERM_COLOR", "never");
  std::print(env.get_or("RUST_BACKTRACE", "unset"));   # 1
};
```

`std::map::empty()` has no argument, so `K` and `V` come from the type the call
is checked against — here the annotation `Map<txt, txt>`. A call uses its
expected type, so `let env: Map<txt, txt> = std::map::empty();` binds both
parameters; without an annotation there is nothing to fix them and the call is
an error. [Generics](/concepts/generics/) covers inference.

`std::map::of(entries)` builds a map from a vec of `Entry` values. The
annotation supplies `K` and `V`, which flow into the argument, so each
`{ key = …, value = … }` literal is an `Entry<txt, txt>` even though the program
never writes `Entry`:

```kiru
use std;
use std::text;
use std::map;
fn main() {
  let m: Map<txt, txt> = std::map::of([
    { key = "a", value = "1" },
    { key = "a", value = "2" },
  ]);
  std::print("@(m.len())");   # 1, the later entry replaced the earlier
};
```

A later entry with the same key replaces an earlier one, so the result has one
entry per distinct key.

## Reading with get

A map is read with `get`, not with a field. `m.get(key)` is the value stored for
`key`; a key the map does not have has no value, so `get` stops the run. When a
key may be absent, `get_or(key, fallback)` returns the value or the fallback:

```kiru
use std;
use std::text;
use std::map;
fn main() {
  let env: Map<txt, txt> = std::map::of([{ key = "A", value = "1" }]);
  std::print(env.get("A"));                  # 1
  std::print(env.get_or("B", "(unset)"));    # (unset)
};
```

`m.key` does not exist — `key` is not a declared field of `Map` — and the one
field it does declare, `entries`, is private, so a map is read only through its
methods.

## Writing with insert

`m.with(key, value)` returns a new map with `value` stored for `key`, replacing
an existing value in place or appending a new key. The map it is called on is
unchanged, so the caller rebinds:

```kiru
use std;
use std::text;
use std::map;
fn main() {
  let env: Map<txt, txt> = std::map::empty();
  env = env.with("RUST_BACKTRACE", "1");        # adds a key
  env = env.with("RUST_BACKTRACE", "0");        # replaces the value, keeping its place
  env = env.with("CARGO_TERM_COLOR", "never");  # adds another key
  std::print(env.get_or("CARGO_TERM_COLOR", "unset"));   # never
};
```

A value passed to a function is a local copy, so `with` cannot change the map it
is called on; returning a new map and rebinding is how a map grows. [Values and
Types](/concepts/values/) covers assignment.

## The rest of the surface

`contains(key)` answers whether a key is present, `len()` is the entry count, and
`keys()` and `values()` return the keys and values in order as vecs:

```kiru
use std;
use std::text;
use std::map;
fn main() {
  let env: Map<txt, txt> = std::map::of([
    { key = "A", value = "1" },
    { key = "B", value = "2" },
  ]);
  std::print("@(env.contains("A")) @(env.len())");   # true 2
  std::print(std::join(env.keys(), ","));        # A,B
  std::print(std::join(env.values(), ","));      # 1,2
};
```

Keys and values keep insertion order. [std::map](/stdlib/map/) has every
signature.

## A map or a declared type?

A `Map<K, V>` fixes only the key and value types, so the set of keys is open.
When the shape is a fixed set of fields whose types differ, declare a `rec`
instead — that is what `std::Output`, `std::cli::Subcommand`, and the other library
shapes are. [Declared Types](/concepts/declared-types/) covers `rec` and `enum`.
