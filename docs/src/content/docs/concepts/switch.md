---
title: Switch
description: Exact text matching, case patterns, and defaults.
---

Kiru has no `if`. To make a decision, you compare text with `switch`. In this
section we'll see how it matches and which patterns it accepts.

`switch` compares a text against case patterns and runs the first arm that
matches:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  txt cmd = "build";
  switch(cmd) {
    case("build") { std::io::print("building"); };
    case("test") { std::io::print("testing"); };
    default { std::io::print("usage: build | test"); };
  };
};
```

## Case patterns

The subject is any text expression. A case pattern is a literal, a name, or a
field path; it names the text to compare and does not compute one:

```kiru
txt fast = "fast";

fn classify(txt mode) -> txt {
  mut txt result = "other";
  switch(mode) {
    case("fast") { result = "literal"; };
    case(fast) { result = "value"; };
  };
  return(result);
};
```

A call, a concatenation, or a literal of another type is an error as a case
pattern:

```console
$ kc main.kiru
main.kiru:3:10: error: a case arm is a literal, a name, or a field path
    case(name()) {};
         ^^^^^^
```

## Matching

Matching is exact text comparison, and the first match wins. `default` is
optional, is not compared, and runs when nothing matches. When nothing matches
and there is no `default`, nothing runs.

Two arms with the same literal or the same name are an error, because the
second can never match:

```console
$ kc main.kiru
main.kiru:4:10: error: duplicate case pattern `a`
    case("a") {};
         ^^^
```

## Arms and scope

Each arm is its own scope. An arm can reassign the enclosing function's `mut`
bindings, but a name declared in one arm does not leak to another. There is no
fallthrough and no arm-level `break`: an arm runs and the switch is done.
