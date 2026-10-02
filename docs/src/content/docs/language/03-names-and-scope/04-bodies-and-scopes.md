---
title: Bodies and Scopes
description: Which blocks share a body, and how threads and cases see bindings.
---

A *body* is a sequence of statements with its own bindings. Function
bodies are bodies, and a thread runs a function, so it runs that function's
body.

## Nested Blocks Share a Body

Every nested block of one function body is the same body for assignment. A
`switch` arm or a `defer` body assigns the bindings of the function it
belongs to:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn pick(txt mode) {
  txt value = "none";
  switch(mode) {
    case("fast") { value = "fast"; };
    default { value = "safe"; };
  };
  return value;
};

fn main() {
  std::print(pick("slow"));
};
```

`value` is declared once, in the function body, and the `default` arm
assigns it, so the program prints `safe`.

## Scope Rules Elsewhere

Each construct adds one scope detail on top of the shared body:

- [Switch](/language/02-common-concepts/09-switch/) covers what a `case` arm
  sees and what stays inside it.
- [Defer](/effects/08-failure-and-cleanup/03-defer/) covers how a `defer` body
  has its own declarations while still assigning the enclosing body's names.
- [Threads](/effects/07-threads/01-threads/) covers the body a thread
  runs and the state it shares with its caller.
