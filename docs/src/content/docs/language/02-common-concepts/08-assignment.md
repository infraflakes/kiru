---
title: Assignment and Read-Only Parameters
description: Redefining a binding, and the values that cannot be assigned.
---

`name = expr;` redefines a `txt` or `rec` binding declared in the enclosing body:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  txt state = "idle";
  state = "running";
  std::print(state);
};
```

The assignment target is a bare name and the value must fit the binding's kind; there is no field assignment, so `env.FOO = expr` is not a statement ([records are immutable](/language/02-common-concepts/02-records/)). A parameter is read-only:

```console
$ kc main.kiru
main.kiru:1:21: error: `spec` is a parameter and is read-only
fn show(rec spec) { spec = "x"; };
                    ^^^^
```

If a body needs a mutable value derived from a parameter, it declares a new `txt`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn normalize(txt name) -> txt {
  txt cleaned = name;
  cleaned = cleaned + "!";
  return(cleaned);
};
```

A top-level binding is a module value and cannot be assigned; [module values](/language/03-names-and-scope/04-module-values/) covers that.
