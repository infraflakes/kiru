---
title: Bindings
description: The binding table, and why the nothing kind cannot be stored.
---

Each kind has exactly one binding form. This is the table the compiler
enforces:

| Kind | Bound by | Notes |
| --- | --- | --- |
| `text` | `txt` | literals, `+`, field access, references, and calls returning text |
| `record` | `rec` | a record literal, a record variable, or a call returning a record |
| `nothing` | nothing | a nothing call; a discarded statement only |

## Text Binds to txt

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt code = std::command({}, "git describe --always");
```

`std::command` yields text, the command's exit code; `txt` stores it. A
result is data, so it can also stand in a record field or pass as an argument
like any other text. [Text](/language/02-common-concepts/01-text/) and
[text concatenation](/language/02-common-concepts/04-text-concatenation/)
cover the text kind.

## Records Bind to rec

<span class="filename">Filename: src/main.kiru</span>

```kiru
rec env = {
  RUST_BACKTRACE = "1",
  CARGO_TERM_COLOR = "never",
};
```

Every field is a text expression. Records do not nest;
[records](/language/02-common-concepts/02-records/) has the full rule. A
record is bound from a record literal, a record variable, or a call returning
a record.

## Nothing Calls Are Never Bound

A `nothing` call is not data, so it cannot be stored:

```console
$ kc main.kiru
main.kiru:1:21: error: expected text, found nothing
fn main() { txt x = std::eprint("hi"); };
                    ^^^^^^^^^^^^^^^^^
```

A `nothing` call stands only as a statement.
