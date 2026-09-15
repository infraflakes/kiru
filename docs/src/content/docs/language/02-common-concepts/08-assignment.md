---
title: Assignment and Mutability
description: Rebinding and field assignment with `mut`.
---

A binding is immutable unless it is declared `mut`. A `mut` binding may be reassigned, and if it holds a record, its fields may be assigned:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  mut txt state = "idle";
  state = "running";

  mut rec env = { RUST_BACKTRACE = "0" };
  env.RUST_BACKTRACE = "1";

  std::print(state + env.RUST_BACKTRACE);
};
```

An assignment target is a bare name and the value must fit the binding's kind. A field assignment is `name.field = expr;`: it replaces one field of a mutable record, and adds the field when it is absent. Assigning a binding that is not `mut` is a compile error:

```console
$ kc main.kiru
main.kiru:2:3: error: `state` is not mutable; declare it `mut`
  state = "running";
  ^^^^^
```

A parameter is immutable unless declared `mut`. A `mut` parameter is a mutable copy: the function may reassign it and assign its fields, and the caller's value is unaffected:

```kiru
fn normalize(mut txt name) -> txt {
  name = name + "!";
  return(name);
};
```

A top-level binding is a module value: it is evaluated once and can never be `mut`; [module values](/language/03-names-and-scope/04-module-values/) covers that.
