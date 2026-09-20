---
title: Declaring and Calling
description: Function syntax, positional arguments, and exact arity.
---

A function is declared with `fn`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn banner(txt name) {
  return("=== " + name + " ===");
};
```

Parameters are comma separated, and each declares its kind before its name:
`txt` accepts text and `rec` accepts a record. A trailing comma is
allowed, and duplicate parameter names are a compile error.

## Calling

`name(arg, ...)` calls a function. The arguments are passed in order:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn banner(txt name) {
  return("=== " + name + " ===");
};

fn main() {
  std::print(banner("kiru"));
};
```

```console
$ ./app
=== kiru ===
```

The argument count must match the parameter count exactly. There are no
default arguments and no optional parameters; a mismatch is a compile error
at the call:

```console
$ kc main.kiru
main.kiru:6:14: error: `banner` takes 1 arguments, found 0
  std::print(banner());
             ^^^^^^
```

Arguments are evaluated left to right before the call. A function cannot be
used as a value; a bare function name in an expression is an error.

A function may end with `return(expr);`, and then it is text or record. A
function with no `return` is void, and its call may only stand as a
statement or as the invocation `std::async` spawns; [return and
void](/language/02-common-concepts/07-return-and-recursion/) covers the
rule.

## Order of Declarations

Functions are read top-down like everything else, so a function must be
declared before its callers. A forward reference is a compile error, and a
function cannot call itself. [Declaration
order](/language/03-names-and-scope/01-declaration-order/) covers visibility.

## Namespaces

A function in another namespace is reached with `::`:

<span class="filename">Filename: main.kiru</span>

```kiru
import "tasks.kiru";

fn main() {
  tasks::clean();
};
```

The namespace is part of the name, so `tasks::clean` and a local `clean`
do not collide. A leading `::` names the root namespace, so `::clean()`
always reaches the root `clean`. [Imports](/language/05-modules-and-namespaces/02-imports/)
covers what an import makes reachable.
