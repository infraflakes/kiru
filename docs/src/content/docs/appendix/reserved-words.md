---
title: Reserved Words
description: The names the language owns.
---

These words are reserved and may not be used as names:

```text
mod  import  use  pub  fn  let  enum  type  rec  match
return  panic  async  wait  for  in  break  true  false
```

plus the `std` namespace.

| Word | Use |
| --- | --- |
| `mod` | declare an inline namespace block |
| `import` | load another file |
| `use` | bring another namespace's public names into this one |
| `pub` | let another namespace reach a declaration, field, or variant |
| `fn` | declare a function |
| `let` | declare a binding |
| `enum` | declare an enum type and its variants |
| `type` | declare a built-in type the runtime provides (a bodyless declaration) |
| `rec` | declare a nominal type (a `rec`) |
| `match` | compare a value |
| `return` | end a function early; `return;` for no return type, `return expr;` for a function with a return type |
| `panic` | exit the program with an error |
| `async` | start a call on its own thread |
| `wait` | join the calling thread's asyncs |
| `for` | repeat a body: once per vec element with `for item in <vec> { ... }`, or forever with `for { ... }` |
| `in` | separate the loop variable from the vec |
| `break` | end the nearest enclosing loop |
| `true` | the true boolean literal |
| `false` | the false boolean literal |

## Everything Else Is Free

`command`, `print`, and `quote` are not reserved; the shipped ones are reached
with `std::`. A program may declare its own `command` function in its own
namespace, and `std::command` remains the shipped one. `rec` declares a nominal
type, and `enum` declares an enum type.

**The built-in type names are not reserved.** `txt`, `Vec`, `bool`, and `int`
are ordinary identifiers: each is a bodyless `type Name<...>;` declaration in a
`std` category (`std::text`, `std::vec`, and `std` itself). They are the
language's own types, so they are visible in every namespace and a program
writes one without a `use`:

```kiru
fn main() {
  let x: int = 1;
  let s: txt = "a";
  let xs: Vec<int> = [1];
};
```

A method still needs its category in scope, so `xs.len()` needs `use std::vec;`:
dispatch keys on the receiver's type head, and the namespace that declares the
method must be reachable.

Because they are not keywords, a program may also use those words for its own
declarations. `Vec` remains the natural name for the shipped type, but nothing
stops a program from declaring a function named `Vec` or `map`. `Map` and
`Entry` are not built-ins at all: `std::map` declares them as ordinary `rec`s
([std::map](/stdlib/map/)).
