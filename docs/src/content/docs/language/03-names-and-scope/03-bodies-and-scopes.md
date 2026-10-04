---
title: Bodies and Scopes
description: Which blocks share a body, and how threads and cases see bindings.
---

A *body* is a sequence of statements with its own bindings. Function bodies are bodies, and a thread runs a function, so it runs that function's body.

## Nested Blocks Share a Body

Every nested block of one function body is the same body for assignment. A `switch` arm assigns the bindings of the function it belongs to:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn pick(txt mode) -> txt {
  txt value = "none";
  switch(mode) {
    case("fast") { value = "fast"; };
    default { value = "safe"; };
  };
  return(value);
};

fn main() {
  std::print(pick("slow"));
};
```

[Switch](/language/02-common-concepts/10-switch/) covers what a `case` arm sees.
