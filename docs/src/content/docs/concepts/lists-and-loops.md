---
title: Lists and Loops
description: List literals, the two for shapes, and break.
---

A list holds several texts in order. In this section we'll write one, run code
once for each element, and control the loop.

A `list` is an ordered sequence of text. A literal uses square brackets:

```kiru
list xs = ["a", "b", "c"];
list empty = [];
```

A list is a type like text and record: a parameter, a return type, and a
binding can all be a list.

## for over a list

`for item in <list> { ... };` runs the body once per element, binding `item` in
order. The item is scoped to the body and immutable:

```kiru
fn first(list xs) -> txt {
  mut txt found = "";
  for x in xs {
    found = x;
    break;
  };
  return(found);
};
```

The iterable must be a list. A list `for` may run zero times.

## for without a list

`for { ... };` repeats until a `break;`:

```kiru
fn main() {
  for {
    txt code = std::process::command({}, "test -f ready");
    switch(code) {
      case("0") { break; };
      default {};
    };
    std::time::sleep("1");
  };
};
```

A plain `for` with no `break` never returns.

## break

`break;` ends the nearest loop. It is an error outside a loop. A loop body is
its own scope.

To skip an element, put the work in a `switch` arm and leave the other arm
empty:

```kiru
for path in paths {
  switch(std::path::ext(path)) {
    case("tmp") {};
    default { std::io::print(path); };
  };
};
```

The standard library reads the OS into lists and joins them back:
`std::text::split`, `std::text::lines`, and `std::text::join`.
