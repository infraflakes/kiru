---
title: Declared Types
description: Fixed shapes with rec and enum, their literals, and field access.
---

The `{ field = value, … }` literal builds a value of a declared type and nothing
else. When a shape should be fixed and its fields should carry their own types,
declare a type. A `rec` names a fixed set of typed fields, and an `enum` names a
fixed set of variants; both are **declared types**.

## rec

A `rec` declaration lists its fields, each `name: Type`:

```kiru
use std;
use std::text;
rec Project {
  name: txt,
  count: int,
  nix: bool,
};
fn main() {
  let kiru: Project = { name = "kiru", count = 3, nix = true };
  std::print("@(kiru.name) has @(kiru.count)");   # kiru has 3
};
```

The value is a `{ … }` literal, and the type comes from the annotation. The
literal never names its type: a return type or a parameter type supplies it too:

```kiru
use std;
use std::text;
rec Project { name: txt, count: int, nix: bool };
fn make() -> Project {
  return { name = "kiru", count = 3, nix = true };
};
fn main() { std::print(make().name); };
```

A missing field, an extra field, or a value of the wrong type is an error, and
because the field set is fixed, a repeated field is always an error too:

```console
$ kc main.kiru
main.kiru:2:30: error: `Project` is missing field `count`
fn main() { let p: Project = { name = "x" }; };
                             ^^^^^^^^^^^^^^
```

A field reads with the field's type, not always text, so `p.count` is an `int`
and can be used in arithmetic. A binding may assign a field a value of the
field's type:

```kiru
use std;
use std::text;
rec Project { name: txt, count: int, nix: bool };
fn bump(p: Project) {
  p.count = p.count + 1;
};
fn main() {
  let p: Project = { name = "kiru", count = 3, nix = true };
  bump(p);
  std::print("@(p.count)");
};
```

A field is a declaration like any other, so it is **private** to the declaring
namespace unless it is marked `pub` (`pub count: int`). A reader in another
namespace reaches a private field through a method, and a `{ … }` literal
written outside the namespace must name only `pub` fields. An enum variant is a
declaration too, and takes `pub` the same way (`pub Ready`).
[Modules](/concepts/modules/) states the rule once.

## enum

An enum is a type with a fixed set of variants. Each variant may carry a
payload, so an enum says a value is "one of these":

```kiru
use std;
enum Shape {
  Dot,
  Circle(int),
  Rect(int, int),
};
fn main() { std::print("shapes"); };
```

A variant is written `Name.Variant` — the same `.` that reads a field. A payload
variant takes its payload in parentheses; a variant with no payload is written
bare:

```kiru
use std;
enum Shape { Dot, Circle(int), Rect(int, int) };
fn main() {
  let dot: Shape = Shape.Dot;
  let circle: Shape = Shape.Circle(2);
  let rect: Shape = Shape.Rect(3, 4);
  std::print("built three shapes");
};
```

A `match` on an enum names a variant arm. Each payload position binds to a fresh
name, or `_` to ignore it:

```kiru
use std;
enum Shape { Dot, Circle(int), Rect(int, int) };
fn area(shape: Shape) -> int {
  let result: int = match shape {
    Shape.Dot => 0;
    Shape.Circle(radius) => radius * radius * 3;
    Shape.Rect(width, height) => width * height;
  };
  return result;
};
fn main() { std::print("@(area(Shape.Rect(3, 4)))"); };
```

A match on an enum must cover every variant, or end with a `_` arm:

```console
$ kc main.kiru
main.kiru:4:20: error: a match that produces a value must cover every case; add a `_` arm
  let color: txt = match c {
                   ^^^^^^^^^
    Color.Red => "red";
^^^^^^^^^^^^^^^^^^^^^^^^
  };
^^^
```

In statement position the `_` arm is optional: when no variant matches, nothing
runs. Two arms that name the same variant are a duplicate, so the second can
never match. A variant pattern on a subject that is not that enum is an error.

## Both are types

A declared type is an ordinary type: it can be a parameter, a return type, a
field, or a vec element (`Vec<Shape>`), and a method can dispatch on it
([Methods](/concepts/methods/) covers that). A declared type may take type
parameters, written `<A, B>` after its name; [Generics](/concepts/generics/)
covers it.

A field read is only for a declared field, and it has that field's declared
type. A lookup in a `Map<K, V>` is a method call, `m.get(key)`, and it has the
value type `V`; [Maps](/concepts/maps/) covers the difference.
