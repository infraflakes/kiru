---
title: Assignment and Read-Only Parameters
description: Redefining a binding, and the values that cannot be assigned.
---

`name = expr;` redefines a `txt` or `rec` binding declared in the
enclosing body:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  txt state = "idle";
  state = "running";
  std::print(state);
};
```

The first statement binds `state` to `idle`; the assignment replaces the
value, so the program prints `running`.

The assignment target is a bare name. There is no field assignment, so
`env.FOO = expr` is not a statement; [records are
immutable](/language/02-common-concepts/02-records/).

## Parameters Are Read-Only

A parameter is a binding, but it cannot be assigned:

```console
$ kc main.kiru
main.kiru:1:21: error: `args` is a parameter and is read-only
fn main(rec args) { args = "x"; };
                    ^^^^
```

Read-only parameters keep a function's contract intact: what the caller
passed is what the body sees. If a body needs a mutable value derived from
a parameter, it declares a new `txt`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn normalize(txt name) {
  txt cleaned = name;
  cleaned = cleaned + "!";
  return cleaned;
};
```

`cleaned` starts as a copy of the parameter and is reassigned freely.

## Module Values Cannot Be Assigned

A top-level `txt` or `rec` is a module value. It is evaluated once at
program startup, and assignment to it is a compile error:

```console
$ kc main.kiru
main.kiru:4:3: error: `title` is a module-level constant and cannot be assigned
  title = "other";
  ^^^^^
```

The initializer runs on the machine that runs the program, once, in
declaration order; a reference reads that one value and never evaluates the
initializer again. [Module
values](/language/03-names-and-scope/05-module-values/) covers that
evaluation.

## What Assignment Does Not Do

Assignment does not declare. The name must already be visible, and the
value must fit the binding's kind: a `txt` stays text, a `rec` stays a
record. Assignment is always written as a statement; it never hides inside
an expression.
