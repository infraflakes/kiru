---
title: Reserved Words
description: The names the language owns.
---

These words are reserved and may not be used as names:

```text
module  import  fn  txt  rec  list  mut  switch  case  default  return
panic  async  wait  for  in  break
```

plus the `std` namespace.

| Word | Use |
| --- | --- |
| `module` | declare a file's namespace |
| `import` | load another file |
| `fn` | declare a function |
| `txt` | bind text; declare a `txt` parameter or return type |
| `rec` | bind a record; declare a `rec` parameter or return type |
| `list` | bind a list; declare a `list` parameter or return type |
| `mut` | make a binding reassignable and its record fields assignable |
| `switch` | compare text |
| `case` | a switch pattern |
| `default` | the fallback arm |
| `return` | end a function early; `return();` for no return type, `return(expr);` for `-> txt`/`-> rec`/`-> list` |
| `panic` | exit the program with an error |
| `async` | start a call on its own thread |
| `wait` | join the calling thread's asyncs |
| `for` | repeat a body: once per list element with `for item in <list> { ... }`, or forever with `for { ... }` |
| `in` | separate the loop variable from the list |
| `break` | end the nearest enclosing loop |

## Everything Else Is Free

`command`, `print`, and `quote` are not reserved; the shipped ones are reached
with `std::`. A program may declare its own `command` function in its own
namespace, and `std::process::command` remains the shipped one. `txt`, `rec`,
and `list` are keywords, but `var` and `record` are ordinary identifiers a
program may use.
