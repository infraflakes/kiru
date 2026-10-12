---
title: Sets
description: The Set type, of and insert, contains and remove, and the comparator it carries.
---

A set is a collection of **distinct keys kept in order**. It is the keys of a
map without the values: `with` puts a key in, `contains` asks whether it is there, and
`without` takes it out, with no duplicate and no associated value. A vec finds an
element by its position; a set finds a key by its order.

`Set` is not a built-in type. It is an ordinary declared type — a `rec` — in the
`std::set` namespace, built over a vec of keys that is kept sorted:

```kiru
use std;
use std::vec;
pub rec Set<K> { entries: Vec<K>, less: fn(K, K) -> bool };
```

`use std::set;` brings `Set` and the set methods into scope. A qualified
`std::set::Set` names the type without it, but a method call like `s.contains(...)`
still needs the `use`, the same as any other method
([Methods](/concepts/methods/) covers reachability). Both of `Set`'s fields,
`entries` and `less`, are **private** to `std::set`: a program never writes
`{ entries = … }` and never reads `s.entries`, and it does not reach for
`s.less` either. A set is built and read only through its methods.
[std::set](/stdlib/set/) lists every signature.

## The order is a comparator you carry

A set keeps its keys sorted, so it has to know how two keys order. Rust writes
that as a bound, `BTreeSet<T: Ord>`, and takes the order from the key's `Ord`.
kiru has no bounds, and the checker rejects ordering a type parameter: a body
that writes `a < b` on two `T` reports that `T` cannot be ordered, and only
`int` or `txt` can be compared. The order cannot come from the type, so it comes
from a value — exactly as `sorted_by` already takes a comparator
([Closures](/concepts/closures/) covers `sorted_by`).

That value is the `less` field: `fn(K, K) -> bool`, answering `true` when its
first argument orders before its second. Every set carries one, and every method
that places a key uses it. `of` takes it as an argument:

```kiru
use std;
use std::text;
use std::set;
use std::vec;
fn main() {
  let s: Set<txt> = std::set::of(["b", "a", "a"], std::vec::text_less);
  std::print(std::join(s.items(), ","));   # a,b
};
```

`of` sorts with the comparator and then drops adjacent duplicates, so one of
each key survives. `std::vec::text_less` is the ready-made byte-order comparator
for text; a set of `int` takes one of its own, and any function of the right
type works. Because the order is a value, a set can order its keys the way the
program wants rather than the way the type forces:

```kiru
use std;
use std::text;
use std::set;
use std::vec;
fn reverse_less(a: txt, b: txt) -> bool { return b < a; };
fn main() {
  let s: Set<txt> = std::set::of(["a", "c", "b"], reverse_less);
  std::print(std::join(s.items(), ","));   # c,b,a
};
```

Two keys are the same when `==` answers `true`; `less` only fixes their order.
The comparator must be consistent — it must order the keys the same way `==`
separates them — or a key could be present yet unreachable.

## Adding, testing, and removing

A set starts from `of` and grows with `with`:

```kiru
use std;
use std::text;
use std::set;
use std::vec;
fn main() {
  let s: Set<txt> = std::set::of(["a", "c"], std::vec::text_less);
  s = s.with("b");
  s = s.with("a");    # already present, so the set is unchanged
  std::print(std::join(s.items(), ","));   # a,b,c
};
```

`with` inserts the key in order and ignores one already present. `contains` answers
whether a key is present, and `without` returns the set without it:

```kiru
use std;
use std::text;
use std::set;
use std::vec;
fn main() {
  let s: Set<txt> = std::set::of(["a", "b", "c"], std::vec::text_less);
  std::print("@(s.contains("b")) @(s.contains("z"))");   # true false
  s = s.without("b");
  std::print(std::join(s.items(), ","));       # a,c
};
```

A value passed to a function is a local copy, so `with` and `without` cannot change
the set they are called on; each returns a new set and the caller rebinds
([Values and Types](/concepts/values/) covers assignment). `with`, `contains`, and
`without` binary-search on `less`, so each is O(log n).

## The set operations

`union`, `intersection`, and `difference` combine two sets, and `is_subset`
relates them. Each merges the two sorted vecs in one pass and orders the result
by the receiver's comparator:

```kiru
use std;
use std::text;
use std::set;
use std::vec;
fn main() {
  let a: Set<txt> = std::set::of(["a", "b"], std::vec::text_less);
  let b: Set<txt> = std::set::of(["b", "c"], std::vec::text_less);
  std::print(std::join(a.union(b).items(), ","));          # a,b,c
  std::print(std::join(a.intersection(b).items(), ","));   # b
  std::print(std::join(a.difference(b).items(), ","));     # a
};
```

`is_subset` is the consistency question a set answers well: every key of this set
is also a key of the other. Together the operations describe what two sets share
and what each has alone — the installed packages against the required ones, the
seen names against the allowed ones:

```kiru
use std;
use std::text;
use std::set;
use std::vec;
fn main() {
  let installed: Set<txt> = std::set::of(["core", "cli", "docs"], std::vec::text_less);
  let required: Set<txt> = std::set::of(["cli", "core"], std::vec::text_less);
  std::print("@(required.is_subset(installed))");                        # true
  std::print(std::join(installed.difference(required).items(), ","));   # docs
  std::print(std::join(installed.intersection(required).items(), ",")); # cli,core
};
```

`len` is the key count and `items` is the keys in order.
[std::set](/stdlib/set/) has every signature.

## A set or a map?

A `Map<K, V>` keeps one value per key and preserves insertion order; a `Set<K>`
keeps the keys alone and keeps them in the comparator's order. When there is no
value to store, a set is the narrower type: it says the keys are distinct and
the operations are the set ones. When a value rides along, use a map. A fixed
set of fields whose types differ is a `rec` instead
([Declared Types](/concepts/declared-types/) covers `rec` and `enum`).
