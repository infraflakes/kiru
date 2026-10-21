---
title: Functions
description: Declaring functions, parameters, returns, and calls.
---

Functions name and reuse a piece of work. A function is declared with `fn`:

```kiru
use std::text;
fn banner(name: txt) -> txt {
  return "=== @(name) ===";
};
fn main() { std::print(banner("kiru")); };
```

Parameters are comma separated, and each writes its name then its type after a
colon. A trailing comma is allowed, and duplicate parameter names are an error.

The return type follows the parameter list: `-> txt`, `-> Vec<txt>`, a
declared type, or nothing at all.

## Parameters

Each parameter's type is written after a colon at its declaration, and every call
is checked against it. A parameter is a local copy: the function may reassign it,
and the caller's value is unchanged. [Values and Types](/concepts/values/) covers
assignment.

## Calling

`name(arg, ...)` calls a function. Arguments are passed in order and evaluated
left to right. The count must match exactly; there are no default or optional
parameters:

```console
$ kc
main.kiru:6:14: error: `banner` takes 1 argument, found 0
  std::print(banner());
             ^^^^^^
```

A call's callee is a name or a method receiver — `f(x)` or `receiver.m(x)` — not
an expression, so `fs[0](x)` and `make()(y)` do not exist. [Methods](/concepts/methods/)
covers the receiver form, and [Closures](/concepts/closures/) covers calling a
function value.

## Returning

`return expr;` ends a function and gives its result. A function declared to
return a value must return on every path:

```kiru
use std::text;
fn pick(flag: txt) -> txt {
  match flag {
    "a" => { return "first"; };
    _ => { return "other"; };
  };
};
fn main() { std::print(pick("a")); };
```

[Match](/concepts/match/) covers the decision above.

A function with no return type may `return;` early. `return` may appear anywhere;
statements after it are unreachable.

## Calls as statements

A statement can be a call, and its result is discarded:

```kiru
use std::text;
fn banner(name: txt) -> txt { return "=== @(name) ==="; };
fn main() {
  std::print("hello");
  banner("kiru");
};
```

A bare statement must be a call. A literal, a name, a `{ … }` literal, or a
concatenation as a statement is an error:

```console
$ kc
main.kiru:2:3: error: a statement must be a call
  "x";
  ^^^
```

A call that returns no value may only stand as a statement: binding, passing,
storing, or returning such a call is an error. [Panic](/failure/panic/) covers
`panic;`, the other bare statement.
