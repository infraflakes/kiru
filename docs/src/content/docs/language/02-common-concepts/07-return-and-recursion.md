---
title: Return and Nothing
description: The return rule, functions that return nothing, and panic.
---

A function ends when its body ends. `return` is an early exit: `return(expr);` ends the function with text or record, and `return();` ends a function with no value.

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn greeting(txt name) {
  return("hello, " + name);
};
```

## The Return Rule

- `return` is optional.
- It may appear anywhere; statements after a terminator are unreachable and are allowed.
- Every value return in one function carries the same kind, and that kind is the function's kind. A function with no return, or whose returns have no common kind, is `nothing`.
- A function with a value return must not fall through: every path must end in `return` or `panic`.
- The returned value must be text or record.

A return may appear before the end of the body:

```kiru
fn label(txt value) {
  return("value: " + value);
  return("unreachable");
};
```

## A Function Without return Returns Nothing

When a function has no value return, it is a `nothing` function, and its call has the `nothing` kind. A `nothing` call may only be a statement:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn note(txt message) {
  std::print(message);
};

fn main() {
  note("hello");
};
```

Binding, passing, storing, or returning it is a compile error:

```console
$ kc main.kiru
main.kiru:2:21: error: expected text, found nothing
fn main() { txt x = note("hi"); };
                    ^^^^^^^^^^
```

A `return` of a `nothing` call is rejected because the returned value must be text or record:

```console
$ kc main.kiru
main.kiru:2:17: error: expected text or record, found nothing
fn f() { return(note("hi")); };
                ^^^^^^^^^^
```

`panic;` is a keyword statement that ends the run. It is a terminator, so a value function may end in it.
