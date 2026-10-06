---
title: Functions
description: Declaring functions, parameters, returns, and calls.
---

Functions are how you name and reuse a piece of work. In this section we'll
declare one, pass arguments, and return a result.

A function is declared with `fn`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn banner(txt name) -> txt {
  return("=== " + name + " ===");
};
```

Parameters are comma separated, and each writes its type before its name. A
trailing comma is allowed, and duplicate parameter names are an error.

The return type follows the parameter list: `-> txt`, `-> rec`, `-> list`, or
nothing at all.

## Parameters

Each parameter's type is written at its declaration, and every call is checked
against it. A parameter is immutable unless declared `mut`; a `mut` parameter is
a mutable copy. [Bindings and
Mutability](/concepts/bindings-and-mutability/) covers `mut`.

## Calling

`name(arg, ...)` calls a function. Arguments are passed in order and evaluated
left to right. The count must match exactly; there are no default or optional
parameters:

```console
$ kc main.kiru
main.kiru:6:14: error: `banner` takes 1 arguments, found 0
  std::io::print(banner());
             ^^^^^^
```

A function cannot be used as a value; a bare function name in an expression is
an error.

## Returning

`return(expr);` ends a function and gives its result. A function declared
`-> txt`, `-> rec`, or `-> list` must return on every path:

```kiru
fn pick(txt flag) -> txt {
  switch(flag) {
    case("a") { return("first"); };
    default { return("other"); };
  };
};
```

A function with no return type may `return();` early. `return` may appear
anywhere; statements after it are unreachable.

## Calls as statements

A statement can be a call, and its result is discarded:

```kiru
fn main() {
  std::io::print("hello");
  banner("kiru");
};
```

A bare statement must be a call. A literal, a name, a record literal, or a
concatenation as a statement is an error:

```console
$ kc main.kiru
main.kiru:2:3: error: a statement must be a call
  "x";
  ^^^
```

A call that returns no value may only stand as a statement. [Panic and
Failure](/failure/panic/) covers `panic;`, the other bare
statement.
