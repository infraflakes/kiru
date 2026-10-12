---
title: Methods
description: Dispatch by receiver type, method families, and chaining.
---

A **method** is a function whose first parameter accepts the receiver. The name
of that parameter is irrelevant; the type after the colon is what makes the call
work. A call `receiver.name(arguments)` resolves the name `name` in scope, picks
the function whose first parameter has the receiver's type, and calls it with the
receiver as the first argument:

```kiru
use std;
use std::vec;
fn count<T>(self: Vec<T>) -> int {
  let n: int = 0;
  for item in self { n = n + 1; };
  return n;
};
fn main() {
  let xs: Vec<int> = [1, 2, 3];
  std::print("@(xs.count())");    # 3
};
```

The parameter is ordinary: `fn count(xs: Vec<T>)` dispatches exactly the same,
because the receiver's type — not the name — selects it. `self` is only a
convention for that first parameter. A method's own type parameters are in scope
for its signature and body, so `count` works on a `Vec<int>` or a vec of any
other element type.

## Targets

The first parameter may be a built-in type (`txt`, `int`, `bool`, `Vec<T>`) or a
declared type:

```kiru
use std;
rec Point { x: int, y: int };

fn sum(self: Point) -> int { return self.x + self.y; };
fn main() {
  let p: Point = { x = 1, y = 2 };
  std::print("@(p.sum())");
};
```

Inside the body, `self` (or whatever the parameter is named) names the receiver,
and it is a local copy, like any parameter.

## Dispatch

A call dispatches on the receiver's type: `xs.count()` on a `Vec<int>` picks the
`count` whose first parameter is a `Vec<T>`. The lookup is by type head — `Vec<T>`
answers a `Vec<int>` receiver — and the receiver's head selects among the
declarations of that name, whether they share one namespace or sit in different
namespaces in scope:

```kiru
use std;
use std::text;   # `len` on txt
use std::vec;    # `len` on Vec<T>
fn main() {
  let xs: Vec<txt> = ["a", "b", "c"];
  let n: int = xs.len();      # picks std::vec::len
  let m: int = "abc".len();   # picks the txt kernel, std::text::len
  std::print("@(n) @(m)");
};
```

Both names are in scope, so the receiver's type chooses. The full argument list
is then checked exactly: a `count` for a `Vec<txt>` called on a `Vec<int>` is
reported as `expected Vec<text>, found Vec<int>`.

After that selection, a method call is an ordinary call: the callee must be
declared earlier, so two methods that call each other are just two functions,
each declared before its caller.

## Reachability

A method is a declaration in a namespace like any other, so it is reached through
the same `use`/`pub` mechanism. To call `"abc".len()`, the namespace that
declares `len` (`std::text`) must be in scope:

```kiru
use std::text;
fn main() {
  let word: txt = "abc";
  std::print("@(word.len())");   # 3
};
```

Without `use std::text;`, the call reports that `len` is declared in `std::text`
and to add the `use`. A method marked `pub` is reachable from a namespace that
`use`s it; a private method is only reachable from its own namespace.

A type's fields are declarations too, so they follow the same rule: a field is
private to its declaring namespace unless it is marked `pub`, and a `{ … }`
literal written elsewhere may name only `pub` fields. A type reached through
methods keeps its fields private — the `std::cli` and `std::command` builders
are examples, with public types whose every read is a method.
[Modules](/concepts/modules/) states the rule once.

## One pool per receiver head

A method is registered in its namespace by name *and* by its receiver's type
head, so a name may repeat once per head. Two methods with the same name in one
namespace are a method family, and the receiver's type head selects between them
exactly as it selects between namespaces:

```kiru
use std;
use std::text;
rec Command { about: txt };
rec Flag { help: txt };

fn about(self: Command, text: txt) -> Command { return self; };
fn about(self: Flag, text: txt) -> Flag { return self; };   # same name, other head
fn main() { std::print("ok"); };
```

Two methods for the same receiver head, or two declarations with the same name
and no receiver, are still `declared more than once in this namespace`. The same
name may also appear in different namespaces (`std::vec::len` and
`std::text::len`), and the receiver's type head chooses between them at the call.

A call whose receiver has no function of that name whose first parameter accepts
it is an error:

```console
$ kc main.kiru
main.kiru:3:6: error: no method `nope` for this value
  xs.nope();
     ^^^^
```
