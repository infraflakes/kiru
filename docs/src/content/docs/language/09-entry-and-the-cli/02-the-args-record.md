---
title: Command Line Arguments
description: The command and flag words, and the record they build.
---

The runtime builds one record from the words after the program name. The
command line is one command word and one flag word:

| Input words | The args record |
| --- | --- |
| `ci` | `cmd = "ci"`, `flag = ""` |
| `ci -profile` | `cmd = "ci"`, `flag = "-profile"` |
| `ci --p` | `cmd = "ci"`, `flag = "--p"` |
| `ci --backend=name` | `cmd = "ci"`, `flag = "--backend=name"` |
| no words | `cmd = ""`, `flag = ""` |
| three or more words | usage error: stderr message, exit 1 |

Both words are copied verbatim; the runtime interprets neither. There is no
`--` handling, no `name=value` parsing, no identifier validation, and no
repeated-flag rule. A flag is whatever string the program decides to accept,
matched with `switch`; [Switch](/language/02-common-concepts/10-switch/) shows
the shape.

A missing word reads as `""`, the missing-key rule from [Field
Access](/language/02-common-concepts/03-field-access/), so a program never
guards a lookup:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main(rec args) {
  std::print("flag: [" + args.flag + "]");
};
```

```console
$ ./app
flag: []
```

The record is built by the runtime and is read-only, so a program cannot
corrupt its own arguments after parsing; [assignment and read-only
parameters](/language/02-common-concepts/08-assignment/) covers the binding
rule.
