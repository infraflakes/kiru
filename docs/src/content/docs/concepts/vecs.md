---
title: Vecs
description: Vec literals, indexing, building, for, and break.
---

A vec holds several values of one type in order. A vec type is written `Vec<T>`,
where `T` is the element type; the element type is always written, so there is
no bare form. A literal uses square brackets:

```kiru
use std;
use std::text;
use std::vec;
fn main() {
  let xs: Vec<txt> = ["a", "b", "c"];
  let empty: Vec<txt> = [];
  let numbers: Vec<int> = [1, 2, 3];
  std::print("@(xs.len())");
};
```

A vec is one of the runtime types, like text and int: a parameter, a return
type, and a binding can all be a vec, and the element can be a declared type:

```kiru
use std;
use std::text;
use std::vec;
rec Task { name: txt, weight: int };

fn heaviest(tasks: Vec<Task>) -> txt {
  let best: txt = "";
  let weight: int = -1;
  for task in tasks {
    match task.weight > weight {
      true => { best = task.name; weight = task.weight; };
      _ => {};
    };
  };
  return best;
};
fn main() { std::print(heaviest([{ name = "a", weight = 2 }])); };
```

[Declared Types](/concepts/declared-types/) covers `rec`.

## Indexing

`xs[i]` reads the element at a zero-based `int` index. The target must be a vec
and the index an `int`; an index at or past the end stops the run:

```kiru
use std::text;
use std::vec;
fn main() {
  let xs: Vec<txt> = ["a", "b", "c"];
  std::print(xs[1]);   # b
};
```

`xs.len()` and `s.len()` return an `int`, so an index and a length compare
directly. `len` works for a vec of any element type.

## Mutable vec

A vec can be written in place with `xs[i] = value`. An index within the vec
replaces the element; an index equal to the length appends:

```kiru
use std;
use std::vec;
fn main() {
  let xs: Vec<int> = [];
  xs[0] = 10;    # appends
  xs[1] = 20;    # appends
  xs[0] = 99;    # replaces
  std::print("@(xs[0]) @(xs[1])");
};
```

An index past the length is an error. This is how a loop builds a vec without
the `with` helper:

```kiru
use std;
use std::vec;
fn squares(count: int) -> Vec<int> {
  let out: Vec<int> = [];
  let i: int = 0;
  for {
    out[i] = i * i;
    i = i + 1;
    match i { count => { break; }; _ => {}; };
  };
  return out;
};
fn main() { std::print("@(squares(3)[2])"); };
```

## for over a vec

`for item in <vec> { ... };` runs the body once per element, binding `item` to
the element type, in order. The item is scoped to the body:

```kiru
use std::text;
use std::vec;
fn head(xs: Vec<txt>) -> txt {
  let found: txt = "";
  for x in xs {
    found = x;
    break;
  };
  return found;
};
fn main() { std::print(head(["a", "b"])); };
```

The iterable must be a vec. A vec `for` may run zero times.

The vec methods are generic over the element type: `len<T>`, `add<T>`,
`add_first<T>`, `concat<T>`, `reverse<T>`, `contains<T>`, `find<T>`,
`unique<T>`, and `last<T>` all work for a vec of any `T`. `find` returns the
first position of an element as an `Option<int>`
([Absence](/concepts/absence/) covers the optional value), and `uniqued` drops
adjacent duplicates, so `sorted` then `uniqued` leaves one of each. The two text-only helpers
stay free functions: `std::join(parts, separator)` joins a `Vec<txt>`, and
`std::vec::sorted(items)` orders a `Vec<txt>`. [Closures](/concepts/closures/)
covers `map`, `filter`, and `sorted_by`, which take a function value.

## for without a vec

`for { ... };` repeats until a `break;`:

```kiru
use std;
fn main() {
  let n: int = 0;
  for {
    n = n + 1;
    match n >= 3 { true => { break; }; _ => {}; };
  };
  std::print("@(n)");   # 3
};
```

A plain `for` with no `break` never returns.

## break

`break;` ends the nearest loop. It is an error outside a loop. A loop body is
its own scope.

To skip an element, put the work in a `match` arm and leave the other arm empty:

```kiru
use std;
use std::text;
use std::vec;
fn main() {
  let paths: Vec<txt> = ["keep.kiru", "skip.tmp"];
  for path in paths {
    match path.ends_with(".tmp") {
      true => {};
      _ => { std::print(path); };
    };
  };
};
```

The standard library reads the OS into vec and joins them back: `std::split`,
`std::text::lines`, and `std::join`.
