---
title: Closures
description: Function values, anonymous functions, and the vec helpers.
---

A function is a value whose type is a function type. The type is written
`fn(parameters) -> result`, and its parameter and return types are checked:

```kiru
use std;

fn double(x: int) -> int {
  return x * 2;
};

fn apply(f: fn(int) -> int, x: int) -> int {
  return f(x);
};

fn main() {
  let g: fn(int) -> int = double;
  std::print("@(apply(g, 21))");    # 42
};
```

A function value is called with `f(arg, ...)`, and the argument count must match
its type. A generic function has no single type, so it cannot be used as a
value. A function where a non-function is expected is an ordinary type mismatch,
not a special rule about functions.

A call's callee is a name or a method receiver, not an expression, so
`fs[0](x)` and `make()(y)` do not exist. Bind the value first and call it by
name:

```kiru
use std;
use std::dynamic::Vec;

fn triple(x: int) -> int { return x * 3; };

fn main() {
  let fs: Vec<fn(int) -> int> = Vec.new();
  fs = fs.push(triple);
  let f: fn(int) -> int = fs.at(0);
  let n: int = f(3);
  std::print("@(n)");   # 9
};
```

## Closures

An anonymous function literal is a value: `fn(a: T) -> R { ... }`. It captures
the enclosing locals by value at the point it is created, so it can use them:

```kiru
use std;
use std::dynamic::Vec;

fn main() {
  let factor: int = 10;
  let xs: Vec<int> = Vec.new();
  xs = xs.push(1);
  xs = xs.push(2);
  xs = xs.push(3);
  let scaled: Vec<int> = xs.map(fn(x: int) -> int { return x * factor; });
  std::print("@(scaled.at(2))");    # 30
};
```

A closure's parameters and return type are written like a function's; with no
return type it is `fn(params)` and returns no value, and its body must return on
every path when it declares a return type. Because the closure holds a copy of
each captured local, it cannot assign one:

```console
$ kc
main.kiru:3:50: error: `total` is captured by a closure and cannot be assigned
  let bump: fn(int) -> int = fn(x: int) -> int { total = total + x; return total; };
                                                 ^^^^^
```

A closure may capture across more than one level, so a closure returned from a
closure still sees the locals of the function that created it.

## The vec helpers

The generic vec methods that take a function value are `map`, which transforms
each element; `filter`, which keeps the elements a predicate accepts; `fold`,
which accumulates from the left; `each`, which calls on every element; `count`,
which counts the matches; and `sort_by`, which orders with a comparator. Each
returns a new vec (except `each`, which returns nothing), so a call chains with
the next ([Vecs](/concepts/vecs/) covers the vec methods):

```kiru
use std;
use std::dynamic::Vec;

fn main() {
  let xs: Vec<int> = Vec.new();
  xs = xs.push(1);
  xs = xs.push(2);
  xs = xs.push(3);
  xs = xs.push(4);
  let doubled: Vec<int> = xs.map(fn(x: int) -> int { return x * 2; });
  let evens: Vec<int> = xs.filter(fn(x: int) -> bool { return x % 2 == 0; });
  let desc: Vec<int> = xs.sort_by(fn(a: int, b: int) -> bool { return a > b; });
  std::print("@(doubled.at(0)) @(evens.at(0)) @(desc.at(0))");   # 2 2 4
};
```

`sort_by` takes `fn(T, T) -> bool` and answers `true` when its first argument
orders before its second. For a `Vec<txt>` in byte order, `Vec.sort` is the
ready-made method: `items.sort()`.
