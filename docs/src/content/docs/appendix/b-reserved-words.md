---
title: Appendix B. Reserved Words
description: The names the language owns.
---

These words are reserved and may not be used as names:

```text
module  import  fn  txt  rec  switch  case  default  defer  return
```

plus the `std` namespace.

## Where Each Word Is Used

| Word | Use |
| --- | --- |
| `module` | declare a file's namespace |
| `import` | load another file |
| `fn` | declare a function |
| `txt` | bind text; declare a text parameter |
| `rec` | bind a record; declare a record parameter |
| `switch` | compare text |
| `case` | a switch pattern |
| `default` | the fallback arm |
| `defer` | register cleanup |
| `return` | exit a function with a text or record value |

## Everything Else Is Free

`command`, `async`, `wait`, `print`, and `panic` are not reserved; the
shipped ones are reached with `std::`. A program may declare its own
`command` function in its own namespace, and `std::command` remains the
native. `txt` and `rec` are keywords, but `var` and `record` are ordinary
identifiers a program may use.
