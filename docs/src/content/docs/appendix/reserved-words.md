---
title: Reserved Words
description: The names the language owns.
---

These words are reserved and may not be used as names:

```text
mod  import  fn  let  txt  rec  list  mut  match  return
panic  async  wait  for  in  break
```

plus the `std` namespace.

| Word | Use |
| --- | --- |
| `mod` | declare an inline namespace block |
| `import` | load another file |
| `fn` | declare a function |
| `let` | declare a binding |
| `txt` | annotate text; declare a `txt` binding, parameter, or return type |
| `rec` | annotate a record; declare a `rec` binding, parameter, or return type |
| `list` | annotate a list; declare a `list` binding, parameter, or return type |
| `mut` | make a binding reassignable and its record fields assignable |
| `match` | compare text |
| `return` | end a function early; `return;` for no return type, `return expr;` for `-> txt`/`-> rec`/`-> list` |
| `panic` | exit the program with an error |
| `async` | start a call on its own thread |
| `wait` | join the calling thread's asyncs |
| `for` | repeat a body: once per list element with `for item in <list> { ... }`, or forever with `for { ... }` |
| `in` | separate the loop variable from the list |
| `break` | end the nearest enclosing loop |

## Everything Else Is Free

`command`, `print`, and `quote` are not reserved; the shipped ones are reached
with `std::`. A program may declare its own `command` function in its own
namespace, and `std::command` remains the shipped one. `txt`, `rec`,
and `list` are keywords, but `var` and `record` are ordinary identifiers a
program may use.
