---
title: Absence
description: Option, none, or, has, and where absence appears.
---

Many helpers can fail to produce a value: an environment variable may be unset,
a needle may not be found. Kiru reports that with `Option<T>`, an ordinary enum
with two variants — `some(value)` when the value is present, and `none` when it
is absent:

```text
pub enum Option<T> { pub some(T), pub none };
```

`Option` is not a built-in type. It is declared in the `std` namespace, in
`std/std.kiru`, and written `std::Option` or brought in with `use std;`.
Both variants are `pub`, so a program outside `std` constructs and matches them
([Namespaces](/concepts/modules/) covers visibility).

## Matching

The value is read by matching the two variants:

```kiru
use std;
use std::text;
fn main() {
  let value: std::Option<int> = "a=b".find("=");
  let index: int = match value {
    std::Option.some(index) => index;
    std::Option.none => -1;
  };
  std::print("@(index)");   # 1
};
```

## or and has

Two methods answer the common cases. `or(fallback)` returns the value
inside `some`, or `fallback` when it is `none`:

```kiru
use std;
use std::text;
fn main() {
  let found: int = "a=b".find("=").or(-1);
  let missing: int = "ab".find("=").or(-1);
  std::print("@(found) @(missing)");   # 1 -1
};
```

`has()` answers whether a value is present:

```kiru
use std;
use std::text;
fn main() {
  let present: bool = "a=b".find("=").contains();
  std::print("@(present)");   # true
};
```

Both are methods on `Option`, declared in `std`, so calling one needs `use std;`
like any other name.

## Where absence appears

The absence shows up wherever a lookup can miss:

- `std::syscalls::getenv(name)` is `Option<txt>`, and `Option.none` when the
  variable is unset ([std::syscalls](/stdlib/syscalls/) covers it);
- `text.find(needle)` is `Option<int>`, and `Option.none` when the needle is
  absent ([Text](/concepts/text/) covers the byte index).

A call that returns no value produces no value at all, so it is not an `Option`;
such a call may only stand as a statement ([Functions](/concepts/functions/)
covers it).

`Option` is built from the same pieces as any declared type: [Declared
Types](/concepts/declared-types/) covers `enum`, and [Generics](/concepts/generics/)
covers the type parameter `T`.
