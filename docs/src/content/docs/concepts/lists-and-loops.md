---
title: Lists and Loops
description: List literals, the two for shapes, and break.
---

A list holds several texts in order. In this section we'll write one, run code
once for each element, and control the loop.

A `list` is an ordered sequence of text. A literal uses square brackets:

```kiru
let xs<list> = ["a", "b", "c"];
let empty<list> = [];
```

A list is a type like text and record: a parameter, a return type, and a
binding can all be a list.

## for over a list

`for item in <list> { ... };` runs the body once per element, binding `item` in
order. The item is scoped to the body and immutable:

```kiru
fn first(xs<list>) -> txt {
  let mut found<txt> = "";
  for x in xs {
    found = x;
    break;
  };
  return found;
};
```

The iterable must be a list. A list `for` may run zero times.

## for without a list

`for { ... };` repeats until a `break;`:

```kiru
fn main() {
  for {
    let code<txt> = std::command({}, "test -f ready");
    match code {
      "0" => { break; };
      _ => {};
    };
    std::time::sleep("1");
  };
};
```

A plain `for` with no `break` never returns.

## break

`break;` ends the nearest loop. It is an error outside a loop. A loop body is
its own scope.

To skip an element, put the work in a `match` arm and leave the other arm
empty:

```kiru
for path in paths {
  match std::path::ext(path) {
    "tmp" => {};
    _ => { std::print(path); };
  };
};
```

The standard library reads the OS into lists and joins them back:
`std::split`, `std::text::lines`, and `std::join`.
