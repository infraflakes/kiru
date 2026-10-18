---
title: Maps
description: The Map type, new and insert, get and get_or, and the ordered association.
---

A map is an **ordered association** from keys to values. It is keyed by
equality: writing a key that is already present replaces its value and keeps its
place, and a new key appends. A vec finds an element by its position; a map finds
a value by its key.

`Map` is not a built-in type. It is an ordinary declared type — a `rec` — in the
`std::dynamic` namespace, built over a `Vec`:

```text
use std::dynamic::Vec;
rec Entry<K, V> { pub key: K, pub value: V };
pub rec Map<K, V> { entries: Vec<Entry<K, V>> };
```

`use std::dynamic;` brings `Map` and the map methods into scope. A qualified
`std::dynamic::Map` names the type without it, and a method call like `m.get(...)`
needs no `use` at all, because a bound function is gated by `pub` alone
([Methods](/concepts/methods/) covers reachability). `Entry` and `Map`'s one
field, `entries`, are **private** to `std::dynamic`: a program never names
`Entry`, never writes `{ entries = … }`, and never reads `m.entries`. A map is
built and read only through its methods. [Map](/stdlib/map/) lists every
signature.

## Building a map

A map starts empty with `Map.new()` and grows with `insert`:

```kiru
use std;
use std::dynamic::Map;

fn main() {
  let env: Map<txt, txt> = Map.new();
  env = env.insert("RUST_BACKTRACE", "1");
  env = env.insert("CARGO_TERM_COLOR", "never");
  std::print(env.get_or("RUST_BACKTRACE", "unset"));   # 1
};
```

`Map.new()` has no argument, so `K` and `V` come from the type the call is
checked against — here the annotation `Map<txt, txt>`. A call uses its expected
type, so `let env: Map<txt, txt> = Map.new();` binds both parameters; without an
annotation there is nothing to fix them and the call is an error.
[Generics](/concepts/generics/) covers inference.

## Reading with get

A map is read with `get`, not with a field. `m.get(key)` is the value stored for
`key`; a key the map does not have has no value, so `get` stops the run. When a
key may be absent, `get_or(key, fallback)` returns the value or the fallback:

```kiru
use std;
use std::dynamic::Map;

fn main() {
  let env: Map<txt, txt> = Map.new();
  env = env.insert("A", "1");
  std::print(env.get("A"));                  # 1
  std::print(env.get_or("B", "(unset)"));    # (unset)
};
```

`m.key` does not exist — `key` is not a declared field of `Map` — and the one
field it does declare, `entries`, is private, so a map is read only through its
methods.

## Writing with insert

`m.insert(key, value)` returns a new map with `value` stored for `key`,
replacing an existing value in place or appending a new key. The map it is
called on is unchanged, so the caller rebinds:

```kiru
use std;
use std::dynamic::Map;

fn main() {
  let env: Map<txt, txt> = Map.new();
  env = env.insert("RUST_BACKTRACE", "1");        # adds a key
  env = env.insert("RUST_BACKTRACE", "0");        # replaces the value, keeping its place
  env = env.insert("CARGO_TERM_COLOR", "never");  # adds another key
  std::print(env.get_or("CARGO_TERM_COLOR", "unset"));   # never
};
```

A value passed to a function is a local copy, so `insert` cannot change the map
it is called on; returning a new map and rebinding is how a map grows.
[Values and Types](/concepts/values/) covers assignment.

## The rest of the surface

`contains(key)` answers whether a key is present, `len()` is the entry count, and
`keys()` and `values()` return the keys and values in order as vecs:

```kiru
use std;
use std::dynamic::{Map, Vec};

fn main() {
  let env: Map<txt, txt> = Map.new();
  env = env.insert("A", "1");
  env = env.insert("B", "2");
  std::print("@(env.contains("A")) @(env.len())");   # true 2
  std::print(env.keys().join(","));                  # A,B
  std::print(env.values().join(","));                # 1,2
};
```

Keys and values keep insertion order. [Map](/stdlib/map/) has every signature.

## A map or a declared type?

A `Map<K, V>` fixes only the key and value types, so the set of keys is open.
When the shape is a fixed set of fields whose types differ, declare a `rec`
instead — that is what `std::process::Output`, `std::cli::Subcommand`, and the
other library shapes are. [Declared Types](/concepts/declared-types/) covers
`rec` and `enum`.
