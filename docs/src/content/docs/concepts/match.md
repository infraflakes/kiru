---
title: Match
description: Exact text matching, patterns, and the default arm.
---

Kiru has no `if`. To make a decision, you compare text with `match`. In this
section we'll see how it matches and which patterns it accepts.

`match` compares a text against patterns and runs the first arm that matches:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  let cmd<txt> = "build";
  match cmd {
    "build" => { std::print("building"); };
    "test" => { std::print("testing"); };
    _ => { std::print("usage: build | test"); };
  };
};
```

## Patterns

The subject is any text expression. A pattern is a literal, a name, or a field
path; it names the text to compare and does not compute one:

```kiru
let fast<txt> = "fast";

fn classify(mode<txt>) -> txt {
  let mut result<txt> = "other";
  match mode {
    "fast" => { result = "literal"; };
    fast => { result = "value"; };
  };
  return result;
};
```

A call, a concatenation, or a literal of another type is an error as a pattern:

```console
$ kc main.kiru
main.kiru:3:5: error: a match arm is a literal, a name, or a field path
    name() => {};
    ^^^^^^
```

## Matching

Matching is exact text comparison, and the first match wins. The `_` arm is the
default: it is optional, is not compared, and runs when nothing matches. When
nothing matches and there is no `_` arm, nothing runs.

Two arms with the same literal or the same name are an error, because the
second can never match:

```console
$ kc main.kiru
main.kiru:4:5: error: duplicate match pattern `a`
    "a" => {};
    ^^^
```

## A match is a value

A match is a term, so its role comes from where it sits. In expression position
its arms are expressions and it yields the taken arm's value:

```kiru
let mode<txt> = "fast";
let label<txt> = match mode {
  "fast" => "quick";
  _ => "slow";
};
```

In statement position its arms are blocks, and it runs for its effect:

```kiru
match mode {
  "fast" => { std::print("quick"); };
  _ => { std::print("slow"); };
};
```

## Arms and scope

Each arm is its own scope. An arm can reassign the enclosing function's `mut`
bindings, but a name declared in one arm does not leak to another. There is no
fallthrough and no arm-level `break`: an arm runs and the match is done.
