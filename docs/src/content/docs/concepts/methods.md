---
title: Methods
description: impl blocks, bound functions, dispatch, and chaining.
---

A **bound function** is a function declared inside an `impl` block for a type.
There are two kinds, told apart by the first parameter:

- a **method** has the block's type as its first parameter, so a call
  `receiver.name(arguments)` passes the receiver as that first argument;
- an **associated function** has any other first parameter, so it is reached as
  `Type.name(arguments)`.

A bound function is never a namespace-level name. It is reached only through its
type: a value call, a `Type.name` call, or `Type.name` as a value.

## impl blocks

`impl Type { fn ...; };` files every function inside under the block's type.
The type is named as a written type is, so `impl Vec` and `impl Command` work
for a declared type.

```kiru
use std;

rec Point { x: int, y: int };

impl Point {
  pub fn sum(self: Point) -> int {
    return self.x + self.y;
  };
};

fn main() {
  let p: Point = { x = 1, y = 2 };
  std::print("@(p.sum())");   # 3
};
```

The first parameter's **type** is the dispatch target; its **name** is
irrelevant. `fn sum(self: Point)` and `fn sum(p: Point)` dispatch exactly the
same. `self` is only a convention for that first parameter. Inside the body,
`self` (or whatever the parameter is named) names the receiver, and it is a
local copy, like any parameter.

An associated function has a first parameter that is not the block's type, or no
parameter at all:

```kiru
use std;

rec Point { x: int, y: int };

impl Point {
  pub fn origin() -> Point {
    return { x = 0, y = 0 };
  };
};

fn main() {
  let p: Point = Point.origin();
  std::print("@(p.x)");   # 0
};
```

`Point.origin()` is a `Type.name` call. The type's name is also how the function
is reached as a value: `Point.origin` is the function itself.

```kiru
use std;

rec Counter { n: int };

impl Counter {
  pub fn zero() -> Counter {
    return { n = 0 };
  };
};

fn make(build: fn() -> Counter) -> Counter {
  return build();
};

fn main() {
  let c: Counter = make(Counter.zero);
  std::print("@(c.n)");   # 0
};
```

## Dispatch

A value call dispatches on the receiver's type head: `xs.len()` on a `Vec<int>`
picks the `len` whose first parameter is a `Vec<T>`, and the arguments are then
checked exactly. A `Type.name` call reads the associated-function pool by the
type's head.

Because a bound function is filed under a type, a name may repeat in a namespace
when the functions are bound to **different types**. `Vec.len` and `HashMap.len`
share `len` in `std::dynamic`, and the receiver's type head selects between
them:

```kiru
use std;

rec A { n: int };
rec B { n: int };

impl A {
  pub fn get(self: A) -> int { return self.n; };
};

impl B {
  pub fn get(self: B) -> int { return self.n + 1; };
};

fn main() {
  let a: A = { n = 1 };
  let b: B = { n = 1 };
  std::print("@(a.get()) @(b.get())");   # 1 2
};
```

Two functions bound to the same type, or two plain declarations of one name, are
still an error. A call whose receiver has no method of that name is an error:

```console
$ kc
main.kiru:3:6: error: no method `nope` for this value
  xs.nope();
     ^^^^
```

## Reachability

A bound function is gated by `pub` alone, never by a `use`. In its own namespace
it is reachable whatever its visibility; across a namespace it is reachable only
when the function is `pub` and its type is `pub`. Calling a method therefore
needs no `use` — the type the receiver has decides:

```kiru
use std;
use std::dynamic::Vec;

fn main() {
  let word: txt = "abc";
  let xs: Vec<txt> = Vec.new();
  xs = xs.push(word);
  std::print("@(xs.len())");   # 1
};
```

`Vec` is reached by its `use`, and `len` comes with the type. A private bound
function is reachable only from its own namespace, so a library keeps its
helpers private and marks the ones a program may call `pub`.

A type's fields are declarations too, so they follow the same rule: a field is
private to its declaring namespace unless it is marked `pub`, and a `{ … }`
literal written elsewhere may name only `pub` fields. A type reached through
methods keeps its fields private — the `std::process` and `std::cli` builders
are examples, with public types whose every read is a bound function.
[Namespaces](/concepts/modules/) states the visibility rule once.

## Chaining

Because each method returns a value, calls chain. A builder is a `rec` whose
methods each return a new copy, so a configuration reads as one chain:

```kiru
use std;

rec Step { line: txt, quiet: bool };

impl Step {
  pub fn quiet(self: Step) -> Step {
    let copy: Step = self;
    copy.quiet = true;
    return copy;
  };

  pub fn run(self: Step) -> int {
    match self.quiet {
      true => { return 0; };
      _ => { return 1; };
    };
  };
};

fn main() {
  let start: Step = { line = "cargo build", quiet = false };
  let code: int = start.quiet().run();
  std::print("@(code)");   # 0
};
```

The `std::process` command builder is exactly this shape:
`command(line).dir(...).run()`. [Running
Commands](/commands/running-a-command/) covers it.
