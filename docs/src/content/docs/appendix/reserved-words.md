---
title: Reserved Words
description: The names the language owns.
---

These words are reserved and may not be used as names:

```text
use  pub  fn  impl  enum  let  rec  match
return  panic  async  wait  for  break  true  false
```

plus the `std` namespace.

| Word | Use |
| --- | --- |
| `use` | shorten a path to a namespace, a type, or a plain function |
| `pub` | let another namespace reach a declaration, field, or variant |
| `fn` | declare a function, or write a function type |
| `impl` | file functions under a type |
| `enum` | declare an enum type and its variants |
| `let` | declare a binding |
| `rec` | declare a nominal type (a `rec`) |
| `match` | compare a value |
| `return` | end a function early; `return;` for no return type, `return expr;` for a function with a return type |
| `panic` | exit the program with an error |
| `async` | start a call on its own thread |
| `wait` | join the calling thread's asyncs |
| `for` | repeat a body until a `break;` |
| `break` | end the nearest enclosing loop |
| `true` | the true boolean literal |
| `false` | the false boolean literal |

## Everything Else Is Free

`command`, `print`, and `quote` are not reserved; the shipped ones are reached
with `std::`. A program may declare its own `command` function in its own
namespace, and `std::process::command` remains the shipped one.

**The intrinsic type names are not reserved.** `txt`, `bool`, and `int` are
ordinary identifiers: they are the runtime's own types, so they are visible in
every namespace and a program writes one without a `use`. `Vec` is not an
intrinsic: it is an ordinary declared `rec` in `std::dynamic`, reached by its
path or with `use std::dynamic::Vec;`:

```kiru
use std;
use std::dynamic::Vec;

fn main() {
  let x: int = 1;
  let s: txt = "a";
  let xs: Vec<int> = Vec.new();
  std::print("@(x) @(s) @(xs.len())");
};
```

A bound function is gated by `pub` alone, so a method call needs no `use`:
dispatch keys on the receiver's type head, and the type and the function decide
reachability.

Because they are not keywords, a program may also use those words for its own
declarations. `Vec` remains the natural name for the shipped type, but nothing
stops a program from declaring a function named `Vec` or `map`. `Map` and `Set`
are not built-ins at all: `std::dynamic` declares them as ordinary `rec`s
([Map](/stdlib/map/) and [Set](/stdlib/set/)).
