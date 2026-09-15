---
title: Return and Nothing
description: The return rule, functions that return nothing, and panic.
---

A function declares its return kind after the parameter list. `-> txt` returns text, `-> rec` returns a record, and no arrow returns nothing:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn greeting(txt name) -> txt {
  return("hello, " + name);
};
```

## The Return Rule

- `return(expr);` ends a function declared `-> txt` or `-> rec` early, and the value must fit the declared kind.
- `return();` ends a function declared with no return kind early.
- `return` may appear anywhere in its function; statements after a terminator are unreachable and are allowed.
- `return` is not allowed inside a `defer` body.
- A function declared `-> txt` or `-> rec` must not fall through: every path must end in `return(expr);`, `panic;`, or a call that stops the run.

A return may appear before the end of the body:

```kiru
fn label(txt value) -> txt {
  return("value: " + value);
  return("unreachable");
};
```

## A Function Without a Return Kind Returns Nothing

When a function has no arrow, it returns nothing, and its call has the `nothing` kind. A `nothing` call may only be a statement:

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

A `return` of a `nothing` call is rejected because the returned value must fit the declared kind:

```console
$ kc main.kiru
main.kiru:2:21: error: expected text, found nothing
fn f() -> txt { return(note("hi")); };
                       ^^^^^^^^^^
```

## Functions That Stop the Run

`panic;` is a keyword statement that ends the run. A function stops the run when every path of its body ends in `panic;` or a call to a function that stops the run. A call to such a function ends its caller's path exactly like `panic;`, so it can end a value function's path:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn fail(txt reason) -> txt {
  std::eprint(reason);
};
```

`std::eprint` writes an `ERROR:` line and then panics, so `fail` stops the run and never falls through.
