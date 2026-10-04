---
title: Declaration Order
description: Declare before use, and what counts as visible.
---

Names are read top-down. A name must be declared before the point that mentions it, and a forward reference is a compile error. Here `main` calls `banner` before it is declared:

```console
$ kc main.kiru
main.kiru:2:15: error: `banner` is declared after this point
  txt title = banner("kiru");
              ^^^^^^
```

## What a Body Sees

A function body sees, in order of closeness, its own parameters, the bindings the body declares itself, and module declarations above the function. It does not see another body's local bindings, and it does not see module declarations below it:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt app = "kiru";          # visible below

fn show() -> txt {
  txt local = "x";         # visible only inside show
  std::print(app + local);
  return("");
};
```

## Rooted Names

An unqualified name climbs outward from the current namespace. A leading `::` starts at the root instead, so a root name a local would shadow stays reachable:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt name = "root";

fn show() {
  txt name = "local";
  std::print(name);      # local
  std::print(::name);    # root
};

fn main() {
  show();
};
```

```console
$ ./app
local
root
```

The same prefix works for a call: `::build()` reaches the root `build` function.

## No Recursion

A function cannot call itself. Its own name is not visible inside its body, so a recursive call is rejected:

```console
$ kc main.kiru
main.kiru:2:3: error: a function cannot reference itself
  down();
  ^^^^
```

A value cannot reference itself either. Because every name must be declared before the point that uses it, a program has no recursion at all and no cycle to resolve.
