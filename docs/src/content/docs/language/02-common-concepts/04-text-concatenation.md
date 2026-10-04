---
title: Text Concatenation
description: Concatenation, and why it is the only operator.
---

`+` concatenates text and is the only operator in the language:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn label(txt name, txt code) {
  return("  " + name + ": " + code + "\n");
};

fn main() {
  std::print(label("clippy", "0"));
};
```

Both operands must be text. Anything else is a compile error:

```console
$ kc main.kiru
main.kiru:1:27: error: expected text, found record
fn main() { txt x = "a" + { b = "c" }; };
                          ^^^^^^^^^^^
```

There is no `==`, `<`, or `!=`; comparison is [switch](/language/02-common-concepts/10-switch/). There is no arithmetic operator; [text](/language/02-common-concepts/01-text/) explains why numbers are text.

## Building Lines Safely

Because `+` only concatenates, a command line is always a string built from strings and the language never parses or reinterprets it. A value that should reach a command as data rather than as shell syntax is escaped; [the runtime builtins](/stdlib/01-runtime-builtins/) cover escaping.
