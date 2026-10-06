---
title: Names and Scope
description: Declaration order, uniqueness, shadowing, and what each body sees.
---

Before we split a program across files, we need to know where a name is
visible. In this section we'll cover declaration order, unique names, and
scopes.

## Declare before use

Names are read top-down. A name must be declared before the point that uses it;
a forward reference is an error:

```console
$ kc main.kiru
main.kiru:2:15: error: `banner` is declared after this point
  txt title = banner("kiru");
              ^^^^^^
```

A function body sees, in order of closeness, its parameters, the bindings it
declares itself, and the declarations above the function. It does not see
another body's locals.

A function cannot call itself, and a value cannot reference itself. Since every
name is declared before its uses, a program has no recursion.

## Rooted names

An unqualified name is looked up in the current namespace, then in each parent.
A leading `::` starts at the root instead, so a root name a local shadows stays
reachable:

```kiru
txt name = "root";

fn show() {
  txt name = "local";
  std::io::print(name);      # local
  std::io::print(::name);    # root
};
```

The same prefix works for a call: `::build()` reaches the root `build`.

## Unique names

A namespace keeps values and functions separately. A value name is declared
once, a function name is declared once, and a function may share a name with a
value:

```kiru
txt thing = "value thing";
fn thing() -> txt { return("function thing"); };
```

A second value or function with the same name is an error:

```console
$ kc main.kiru
main.kiru:2:5: error: `title` is declared more than once in this namespace
txt title = "b";
    ^^^^^
```

Within one function body, a local cannot duplicate a name already local in that
body, but it may shadow a top-level name.

## Scopes

Each function body is a scope. Each `case` arm and each loop body is its own
scope too: a name declared in one arm is not visible in another, and a name
declared in a loop body is not visible after the loop.

Nested blocks still share the enclosing function's bindings for assignment, so
an arm or a loop body can reassign a `mut` binding declared outside it.
[Switch](/concepts/switch/) and [Lists and
Loops](/concepts/lists-and-loops/) show the shape.
