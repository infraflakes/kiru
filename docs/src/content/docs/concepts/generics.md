---
title: Generics
description: Type parameters on declared types and functions, and inference.
---

A declared type may take type parameters, written `<A, B>` after its name:

```kiru
use std;
use std::text;
rec Pair<A, B> {
  first: A,
  second: B,
};

enum Option<T> {
  some(T),
  none,
};
fn main() { std::print("declared"); };
```

Inside the declaration, `A`, `B`, and `T` are types: a field or a variant
payload may use them.

## Using a parameterized type

Write the arguments after the name. Every parameter needs an argument:

```kiru
use std;
use std::text;
rec Pair<A, B> { first: A, second: B };
enum Option<T> { some(T), none };
fn main() {
  let pair: Pair<int, txt> = { first = 1, second = "cargo" };
  std::print(pair.second);   # cargo

  let value: Option<int> = Option.some(5);
  let inner: int = match value { Option.some(x) => x; Option.none => 0; };
  std::print("@(inner)");
};
```

An argument may itself be a parameterized type, so generic types nest:

```kiru
use std;
use std::text;
rec Pair<A, B> { first: A, second: B };
fn main() {
  let nested: Pair<txt, Pair<int, int>> = {
    first = "x",
    second = { first = 1, second = 2 },
  };
  std::print("@(nested.second.second)");   # 2
};
```

A generic type is always applied: `Pair` on its own is an error, and the
argument count must match the declaration.

## Construction and matching

A `{ … }` literal is checked against the expected type, so its fields take the
substituted types:

```kiru
use std;
use std::text;
rec Pair<A, B> { first: A, second: B };
fn main() {
  let pair: Pair<int, txt> = { first = 1, second = "a" };   # first is int, second is txt
  std::print("@(pair.first)");
};
```

An enum variant is constructed and matched the same way. A variant pattern binds
each payload with its substituted type:

```kiru
use std;
use std::text;
enum Option<T> { some(T), none };

fn or(value: Option<int>, fallback: int) -> int {
  let result: int = match value {
    Option.some(x) => x;      # x is int
    Option.none => fallback;
  };
  return result;
};
fn main() { std::print("@(or(Option.some(5), 0))"); };
```

Because a variant's payload type is only known once the type arguments are, a
construction like `Option.some(5)` needs an expected type to fix `T`: an
annotation, a parameter, or a return type. On its own it is an error.

## Generic functions

A function may take type parameters too, after its name. The call infers them
from the arguments:

```kiru
use std;
use std::text;
use std::vec;
fn identity<T>(x: T) -> T { return x; };

fn length<T>(xs: Vec<T>) -> int {
  let count: int = 0;
  for item in xs { count = count + 1; };
  return count;
};
fn main() {
  std::print("@(identity(5))");        # T is int
  std::print(identity("hi"));          # T is txt
  std::print("@(length([1, 2, 3]))");  # T is int, from the vec literal
};
```

Inference unifies each parameter type against its argument's type and solves the
parameters, and the type expected at the call site settles any the arguments
leave open — `let xs: Vec<int> = make();` binds `T` with no argument to infer it
from. Two arguments that disagree (`same(1, "x")`) are an error, and a parameter
neither the arguments nor the expected type settles cannot be inferred. Inside
the body, `T` is a type, so a binding annotated `: T`, a vec `[x]`, and a return
all work. The standard library's `std::vec::len` is exactly such a generic
helper.

A binding's type is written, not inferred, and a generic call is the one place a
type is inferred. [Values and Types](/concepts/values/) states the rule once.
