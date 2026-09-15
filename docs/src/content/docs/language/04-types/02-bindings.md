---
title: Bindings
description: The binding table, and why the nothing kind cannot be stored.
---

Each kind has exactly one binding form. This is the table the compiler enforces:

| Kind | Bound by | Notes |
| --- | --- | --- |
| `text` | `txt` | literals, `+`, field access, references, and calls returning text |
| `record` | `rec` | a record literal, a record variable, or a call returning a record |
| `nothing` | nothing | a nothing call; a discarded statement only |

[Text](/language/02-common-concepts/01-text/), [records](/language/02-common-concepts/02-records/), and [return and nothing](/language/02-common-concepts/07-return-and-recursion/) cover each kind, and [type checking](/language/04-types/03-type-checking/) covers the pass that enforces the table.
