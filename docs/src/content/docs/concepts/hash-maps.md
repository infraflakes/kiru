---
title: Hash maps
description: The HashMap type, new and insert, get and get_or, removal, unordered keys, and building a set yourself.
---

A hash map is an **association** from keys to values. It is keyed by equality:
writing a key that is already present replaces its value, and a new key adds an
entry. A vec finds an element by its position; a hash map finds a value by its
key, in constant time whatever the size of the map.

`HashMap<K, V>` is a declared `rec` in `std::dynamic` — a built-in named type,
not an intrinsic. The runtime keeps its value as a table, so it has no fields
and no literal: a map is built and read only through its methods.

`use std::dynamic;` brings `HashMap` and its methods into scope. A qualified
`std::dynamic::HashMap` names the type without it, and a method call like
`m.get(...)` needs no `use` at all, because a bound function is gated by `pub`
alone ([Methods](/concepts/methods/) covers reachability).
[HashMap](/stdlib/hashmap/) lists every signature.

## Building a map

A map starts empty with `HashMap.new()` and grows with `insert`:

```kiru
use std;
use std::dynamic::HashMap;

fn main() {
  let env: HashMap<txt, txt> = HashMap.new();
  env = env.insert("RUST_BACKTRACE", "1");
  env = env.insert("CARGO_TERM_COLOR", "never");
  std::print(env.get_or("RUST_BACKTRACE", "unset"));   # 1
};
```

`HashMap.new()` has no argument, so `K` and `V` come from the type the call is
checked against — here the annotation `HashMap<txt, txt>`. A call uses its
expected type, so `let env: HashMap<txt, txt> = HashMap.new();` binds both
parameters; without an annotation there is nothing to fix them and the call is
an error. [Generics](/concepts/generics/) covers inference.

## Reading with get

A map is read with `get`, not with a field. `m.get(key)` is the value stored for
`key`; a key the map does not have has no value, so `get` stops the run. When a
key may be absent, `get_or(key, fallback)` returns the value or the fallback:

```kiru
use std;
use std::dynamic::HashMap;

fn main() {
  let env: HashMap<txt, txt> = HashMap.new();
  env = env.insert("A", "1");
  std::print(env.get("A"));                  # 1
  std::print(env.get_or("B", "(unset)"));    # (unset)
};
```

A map has no declared field to read, so `m.key` does not exist and a map is read
only through its methods. `contains(key)` answers whether a key is present
without stopping the run.

## Writing with insert

`m.insert(key, value)` returns a new map with `value` stored for `key`,
replacing an existing value or adding a new key. The map it is called on is
unchanged, so the caller rebinds:

```kiru
use std;
use std::dynamic::HashMap;

fn main() {
  let env: HashMap<txt, txt> = HashMap.new();
  env = env.insert("RUST_BACKTRACE", "1");        # adds a key
  env = env.insert("RUST_BACKTRACE", "0");        # replaces the value
  env = env.insert("CARGO_TERM_COLOR", "never");  # adds another key
  std::print("@(env.len())");                     # 2
};
```

A value passed to a function is a local copy, so `insert` cannot change the map
it is called on; returning a new map and rebinding is how a map grows.
[Values and Types](/concepts/values/) covers assignment.

`m.remove(key)` is the other direction: it returns a new map without that key,
and a key that is absent leaves the map as it was.

## Keys are unordered

A hash map stores its keys by their hash, so it has no first key and no last
key. `keys()` and `values()` return vecs in the table's own order: the two calls
line up for the same map value, but that order is not insertion order and no
program should depend on it:

```kiru
use std;
use std::dynamic::HashMap;
use std::dynamic::Vec;

fn main() {
  let env: HashMap<txt, txt> = HashMap.new();
  env = env.insert("A", "1");
  env = env.insert("B", "2");
  let keys: Vec<txt> = env.keys().sort();
  std::print(keys.join(","));   # A,B
};
```

When output must be stable, sort what you read, as above. `sort()` orders a
`Vec<txt>` in byte order; `sort_by` takes any comparator.

## A set without a Set type

Kiru has no `Set` type. A set is a vec with duplicate values dropped, or a hash
map whose values are a membership flag.

```kiru
use std;
use std::dynamic::Vec;

fn main() {
  let tags: Vec<txt> = Vec.new().push("b").push("a").push("b");
  let unique: Vec<txt> = tags.sort().dedup();
  std::print(unique.join(","));   # a,b
};
```

`dedup()` drops adjacent duplicates, so sort first. When the set is large and
the order never matters, `HashMap<txt, bool>` gives a constant-time
`contains(key)` instead of a linear scan.

## A map or a declared type?

A `HashMap<K, V>` fixes only the key and value types, so the set of keys is
open. When the shape is a fixed set of fields whose types differ, declare a
`rec` instead — that is what `std::process::Output`, `std::cli::Subcommand`, and
the other library shapes are. [Declared Types](/concepts/declared-types/) covers
`rec` and `enum`.
