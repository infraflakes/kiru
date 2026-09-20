---
title: Bindings
description: The binding table, and why commands and markers cannot be stored.
---

Each kind has exactly one binding form. This is the table the compiler
enforces:

| Kind | Bound by | Notes |
| --- | --- | --- |
| `text` | `txt` | literals, `+`, field access, references, `.out()`, `.code()`, and calls returning text |
| `record` | `rec` | fields are text expressions only |
| `command` | nothing | chain-only |
| `nothing` | nothing | a void call; a discarded statement only |
| `never` | - | `std::panic()` |

## Text Binds to txt

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt version = std::command("git describe --always").out();
```

The terminal runs the command and yields text; `txt` stores it. A terminal
result is data, so it can also stand in a record field or pass as an
argument like any other text. [Text](/language/02-common-concepts/01-text/) and
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

Every field is a text expression. Records do not nest and do not hold
command chains; [records](/language/02-common-concepts/02-records/) has the full
rule.

## Commands Are Never Bound

A command exists only inside one expression chain, as a statement. Binding
one with `txt` is a compile error:

```console
$ kc main.kiru
main.kiru:2:11: error: expected text, found command
  txt x = std::command("ls");
          ^^^^^^^^^^^^^^^^^^
```

The rule removes a class of mistakes: there is no command value to misuse
later, no command stored in a record, no field access on a chain. A command
is not data yet, so the language refuses to treat it as data in a binding.
A function that needs a configured command runs it, as [building a
command](/effects/06-commands/01-building-a-command/) shows.

## Void Calls Are Never Bound

A void call is `nothing`, so it cannot be stored:

```console
$ kc main.kiru
main.kiru:1:21: error: expected text, found nothing
fn main() { txt x = std::eprint("hi"); };
                    ^^^^^^^^^^^^^^^^^
```

A `never` call is rejected the same way:

```console
$ kc main.kiru
main.kiru:1:21: error: expected text, found never
fn main() { txt x = std::panic(); };
                    ^^^^^^^^^^^^
```

A `nothing` or `never` call stands only as a statement.
