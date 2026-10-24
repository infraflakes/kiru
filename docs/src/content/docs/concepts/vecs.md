---
title: Vecs
description: The Vec type, its operations, loops, and break.
---

A vec holds several values of one type in order. The type is written `Vec<T>`,
where `T` is the element type; the element type is always written, so there is
no bare form. `Vec` is a declared `rec` in `std::dynamic`, not an intrinsic, so
a program reaches it by its path or with `use std::dynamic::Vec;`. It has no
literal and no index syntax: `[...]` is only a closure's parameter list, so a
value is built and changed through its operations.

```kiru
use std;
use std::dynamic::Vec;
fn main() {
  let xs: Vec<txt> = Vec.new();
  xs = xs.push("a");
  xs = xs.push("b");
  xs = xs.push("c");
  std::print("@(xs.len())");   # 3
};
```

A vec is an ordinary declared type: a parameter, a return type, and a binding
can all be a vec, and the element can be a declared type:

```kiru
use std;
use std::dynamic::Vec;
rec Task { name: txt, weight: int };

fn heaviest(tasks: Vec<Task>) -> txt {
  let best: txt = "";
  let weight: int = -1;
  let index: int = 0;
  for {
    match index < tasks.len() { false => { break; }; true => {}; };
    let task: Task = tasks.at(index);
    match task.weight > weight {
      true => { best = task.name; weight = task.weight; };
      _ => {};
    };
    index = index + 1;
  };
  return best;
};
fn main() {
  let tasks: Vec<Task> = Vec.new();
  tasks = tasks.push({ name = "a", weight = 2 });
  tasks = tasks.push({ name = "b", weight = 5 });
  std::print(heaviest(tasks));   # b
};
```

[Declared Types](/concepts/declared-types/) covers `rec`.

## Reading an element

`xs.at(i)` reads the element at a zero-based `int` index. The target must be a
vec and the index an `int`; an index at or past the end stops the run:

```kiru
use std;
use std::dynamic::Vec;
fn main() {
  let xs: Vec<txt> = Vec.new();
  xs = xs.push("a");
  xs = xs.push("b");
  xs = xs.push("c");
  std::print(xs.at(1));   # b
};
```

`xs.len()` returns an `int`, so an index and a length compare directly. `len`
works for a vec of any element type.

## Building a vec

A vec is built with `Vec.new()` and grown with `push`. `replace(i, value)`
replaces the element at `i`:

```kiru
use std;
use std::dynamic::Vec;
fn main() {
  let xs: Vec<int> = Vec.new();
  xs = xs.push(10);
  xs = xs.push(20);
  xs = xs.replace(0, 99);
  std::print("@(xs.at(0)) @(xs.at(1))");   # 99 20
};
```

`push` appends; an index outside the vec is an error. A loop builds a vec one
element at a time:

```kiru
use std;
use std::dynamic::Vec;
fn squares(count: int) -> Vec<int> {
  let out: Vec<int> = Vec.new();
  let i: int = 0;
  for {
    match i < count { false => { break; }; true => {}; };
    out = out.push(i * i);
    i = i + 1;
  };
  return out;
};
fn main() { std::print("@(squares(3).at(2))"); };   # 4
```

## Looping over a vec

There is no `for x in xs`. A vec is walked with a plain `for { ... };` loop, an
index, and `len`:

```kiru
use std;
use std::dynamic::Vec;
fn total(xs: Vec<int>) -> int {
  let sum: int = 0;
  let index: int = 0;
  for {
    match index < xs.len() { false => { break; }; true => {}; };
    sum = sum + xs.at(index);
    index = index + 1;
  };
  return sum;
};
fn main() {
  let xs: Vec<int> = Vec.new();
  xs = xs.push(1);
  xs = xs.push(2);
  xs = xs.push(3);
  std::print("@(total(xs))");   # 6
};
```

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
use std::dynamic::Vec;
fn main() {
  let paths: Vec<txt> = Vec.new();
  paths = paths.push("keep.kiru");
  paths = paths.push("skip.tmp");
  let index: int = 0;
  for {
    match index < paths.len() { false => { break; }; true => {}; };
    let path: txt = paths.at(index);
    match path.ends_with(".tmp") {
      true => {};
      _ => { std::print(path); };
    };
    index = index + 1;
  };
};
```

## The vec methods

The vec methods are generic over the element type: `len`, `at`, `push`,
`replace`, `insert`, `remove`, `is_empty`, `clear`, `slice`, `take`, `skip`,
`fold`, `each`, `count`, `concat`, `reverse`, `contains`, `index_of`, `dedup`,
`map`, `filter`, `first`, `last`, and `sort_by` all work for a vec of any `T`.
`index_of` returns the first position of an element as an `Option<int>`
([Absence](/concepts/absence/) covers the optional value), and `dedup` drops
adjacent duplicates, so `sort` then `dedup` leaves one of each. Two methods
apply to `Vec<txt>` alone and stay bound to it: `sort()` orders in byte order,
and `join(separator)` builds one text:

```kiru
use std;
use std::dynamic::Vec;
fn main() {
  let xs: Vec<txt> = Vec.new();
  xs = xs.push("b");
  xs = xs.push("a");
  xs = xs.push("a");
  let unique: Vec<txt> = xs.sort().dedup();
  std::print(unique.join(","));   # a,b
};
```

[Closures](/concepts/closures/) covers `map`, `filter`, `fold`, `each`, and
`sort_by`, which take a function value. The standard library reads the OS into a vec and joins it
back: `std::fs::glob`, `std::text::lines`, and `Vec.join`.
