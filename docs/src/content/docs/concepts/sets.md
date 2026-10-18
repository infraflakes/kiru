---
title: Sets
description: The Set type, new and insert, contains and remove, and the comparator it carries.
---

A set is a collection of **distinct keys kept in order**. It is the keys of a
map without the values: `insert` puts a key in, `contains` asks whether it is
there, and `remove` takes it out, with no duplicate and no associated value. A
vec finds an element by its position; a set finds a key by its order.

`Set` is not a built-in type. It is an ordinary declared type — a `rec` — in the
`std::dynamic` namespace, built over a vec of keys that is kept sorted:

```text
use std;
use std::dynamic::Vec;
pub rec Set<K> { entries: Vec<K>, less: fn(K, K) -> bool };
```

`use std::dynamic;` brings `Set` and the set methods into scope. A qualified
`std::dynamic::Set` names the type without it, and a method call like
`s.contains(...)` needs no `use` ([Methods](/concepts/methods/) covers
reachability). Both of `Set`'s fields, `entries` and `less`, are **private** to
`std::dynamic`: a program never writes `{ entries = … }` and never reads
`s.entries`, and it does not reach for `s.less` either. A set is built and read
only through its methods. [Set](/stdlib/set/) lists every signature.

## The order is a comparator you carry

A set keeps its keys sorted, so it has to know how two keys order. Rust writes
that as a bound, `BTreeSet<T: Ord>`, and takes the order from the key's `Ord`.
kiru has no bounds, and the checker rejects ordering a type parameter: a body
that writes `a < b` on two `T` reports that `T` cannot be ordered, and only
`int` or `txt` can be compared. The order cannot come from the type, so it comes
from a value — exactly as `sort_by` already takes a comparator
([Closures](/concepts/closures/) covers `sort_by`).

That value is the `less` field: `fn(K, K) -> bool`, answering `true` when its
first argument orders before its second. Every set carries one, and every method
that places a key uses it. `Set.new(less)` starts an empty set with it:

```kiru
use std;
use std::dynamic::Set;

fn main() {
  let s: Set<txt> = Set.new(std::dynamic::text_order);
  s = s.insert("a");
  s = s.insert("c");
  s = s.insert("b");
  std::print(s.items().join(","));   # a,b,c
};
```

`std::dynamic::text_order` is the ready-made byte-order comparator for text; a
set of `int` takes one of its own, and any function of the right type works.
Because the order is a value, a set can order its keys the way the program wants
rather than the way the type forces:

```kiru
use std;
use std::dynamic::Set;

fn reverse_less(a: txt, b: txt) -> bool { return b < a; };

fn main() {
  let s: Set<txt> = Set.new(reverse_less);
  s = s.insert("a");
  s = s.insert("c");
  s = s.insert("b");
  std::print(s.items().join(","));   # c,b,a
};
```

Two keys are the same when `==` answers `true`; `less` only fixes their order.
The comparator must be consistent — it must order the keys the same way `==`
separates them — or a key could be present yet unreachable.

## Adding, testing, and removing

`insert` adds the key in order and ignores one already present. `contains`
answers whether a key is present, and `remove` returns the set without it:

```kiru
use std;
use std::dynamic::Set;

fn main() {
  let s: Set<txt> = Set.new(std::dynamic::text_order);
  s = s.insert("a");
  s = s.insert("b");
  s = s.insert("c");
  s = s.insert("a");    # already present, so the set is unchanged
  std::print("@(s.contains("b")) @(s.contains("z"))");   # true false
  s = s.remove("b");
  std::print(s.items().join(","));                       # a,c
};
```

A value passed to a function is a local copy, so `insert` and `remove` cannot
change the set they are called on; each returns a new set and the caller rebinds
([Values and Types](/concepts/values/) covers assignment). `insert`, `contains`,
and `remove` binary-search on `less`, so each is O(log n).

## The set operations

`union`, `intersection`, and `difference` combine two sets, and `is_subset`
relates them. Each merges the two sorted vecs in one pass and orders the result
by the receiver's comparator:

```kiru
use std;
use std::dynamic::Set;

fn main() {
  let a: Set<txt> = Set.new(std::dynamic::text_order).insert("a").insert("b");
  let b: Set<txt> = Set.new(std::dynamic::text_order).insert("b").insert("c");
  std::print(a.union(b).items().join(","));          # a,b,c
  std::print(a.intersection(b).items().join(","));   # b
  std::print(a.difference(b).items().join(","));     # a
};
```

`is_subset` is the consistency question a set answers well: every key of this set
is also a key of the other. Together the operations describe what two sets share
and what each has alone — the installed packages against the required ones, the
seen names against the allowed ones:

```kiru
use std;
use std::dynamic::Set;

fn main() {
  let installed: Set<txt> = Set.new(std::dynamic::text_order).insert("core").insert("cli").insert("docs");
  let required: Set<txt> = Set.new(std::dynamic::text_order).insert("cli").insert("core");
  std::print("@(required.is_subset(installed))");                        # true
  std::print(installed.difference(required).items().join(","));          # docs
  std::print(installed.intersection(required).items().join(","));        # cli,core
};
```

`len` is the key count and `items` is the keys in order.
[Set](/stdlib/set/) has every signature.

## A set or a map?

A `Map<K, V>` keeps one value per key and preserves insertion order; a `Set<K>`
keeps the keys alone and keeps them in the comparator's order. When there is no
value to store, a set is the narrower type: it says the keys are distinct and
the operations are the set ones. When a value rides along, use a map. A fixed
set of fields whose types differ is a `rec` instead
([Declared Types](/concepts/declared-types/) covers `rec` and `enum`).
