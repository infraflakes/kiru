---
title: Declaring and Calling
description: Function syntax, positional arguments, and exact arity.
---

A function is declared with `fn`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn banner(txt name) -> txt {
  return("=== " + name + " ===");
};
```

Parameters are comma separated, and each declares its kind before its name: `txt` accepts text and `rec` accepts a record. A trailing comma is allowed, and duplicate parameter names are a compile error.

## Calling

`name(arg, ...)` calls a function. The arguments are passed in order:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn banner(txt name) -> txt {
  return("=== " + name + " ===");
};

fn main() {
  std::print(banner("kiru"));
};
```

The argument count must match the parameter count exactly. There are no default arguments and no optional parameters; a mismatch is a compile error at the call:

```console
$ kc main.kiru
main.kiru:6:14: error: `banner` takes 1 arguments, found 0
  std::print(banner());
             ^^^^^^
```

Arguments are evaluated left to right before the call. A function cannot be used as a value; a bare function name in an expression is an error.

A function declares its return kind after the parameter list: `-> txt` returns text, `-> rec` returns a record, and no arrow returns nothing. A function that returns nothing has a call that may only stand as a statement; [return and nothing](/language/02-common-concepts/07-return-and-recursion/) covers the rule.

A function must be declared before its callers; [declaration order](/language/03-names-and-scope/01-declaration-order/) covers visibility and recursion.
