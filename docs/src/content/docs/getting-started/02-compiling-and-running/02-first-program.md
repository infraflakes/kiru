---
title: A First Program
description: Write, compile, and run the smallest Kiru program.
---

Here is a complete program:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::print("hello");
};
```

```console
$ kc src/main.kiru
$ ./src/main
hello
```

`fn main` is the entry point and `std::print` writes a line; the rest of the
book explains both in full.
