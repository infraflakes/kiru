---
title: Bindings and Mutability
description: Local bindings, mut, reassignment, and mutable parameter copies.
---

A binding gives a name to a value. By default the name cannot be changed; in
this section we'll see how to make one that can.

A binding is immutable unless it is declared `mut`:

```kiru
fn main() {
  mut txt state = "idle";
  state = "running";

  mut rec env = { RUST_BACKTRACE = "0" };
  env.RUST_BACKTRACE = "1";

  std::io::print(state + env.RUST_BACKTRACE);
};
```

`name = expr;` reassigns a `mut` binding, and the value must have the binding's
type. Assigning a binding that is not `mut` is an error:

```console
$ kc main.kiru
main.kiru:2:3: error: `state` is not mutable; declare it `mut`
  state = "running";
  ^^^^^
```

`name.field = expr;` assigns one field of a `mut` record, adding the field when
it is absent. [Records](/concepts/records/) covers fields.

## Mutable parameters

A parameter is immutable unless declared `mut`. A `mut` parameter is a mutable
copy: the function can reassign it and assign its fields, and the caller's value
is unchanged:

```kiru
fn normalize(mut txt name) -> txt {
  name = name + "!";
  return(name);
};
```

Top-level values can never be `mut`; [Program
Structure](/concepts/namespaces/) covers them.
