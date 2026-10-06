---
title: Appendix B. Reserved Words
description: The names the language owns.
---

These words are reserved and may not be used as names:

```text
module  import  fn  txt  rec  mut  switch  case  default  defer  return
panic  async  wait
```

plus the `std` namespace.

## Where Each Word Is Used

| Word | Use |
| --- | --- |
| `module` | declare a file's namespace |
| `import` | load another file |
| `fn` | declare a function |
| `txt` | bind text; declare a `txt` parameter or return type |
| `rec` | bind a record; declare a `rec` parameter or return type |
| `mut` | make a binding reassignable and its record fields assignable |
| `switch` | compare text |
| `case` | a switch pattern |
| `default` | the fallback arm |
| `defer` | register cleanup |
| `return` | end a function early; `return();` for no return kind, `return(expr);` for `-> txt`/`-> rec` |
| `panic` | end the run |
| `async` | start a call on its own thread |
| `wait` | join the calling thread's asyncs |

## Everything Else Is Free

`command`, `print`, and `quote` are not reserved; the shipped ones are reached
with `std::`. A program may declare its own `command` function in its own
namespace, and `std::command` remains the shipped one. `txt` and `rec` are
keywords, but `var` and `record` are ordinary identifiers a program may use.
