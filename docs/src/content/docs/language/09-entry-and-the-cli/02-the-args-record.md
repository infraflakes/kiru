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

## Parsing

The first word becomes `cmd`, the second becomes `flag`:

```text
ci --backend=name
  cmd  = "ci"
  flag = "--backend=name"
```

Both words are copied verbatim; the runtime interprets neither. There is no
`--` handling, no `name=value` parsing, no identifier validation, and no
repeated-flag rule. A flag is whatever string the program decides to
accept, matched with the nested `switch` shape shown in [the entry
function](/language/09-entry-and-the-cli/01-main/).

## Missing Words

`cmd` and `flag` read as `""` when the word is absent, so a program never
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

A third word is a usage error before `main` runs:

```console
$ ./app ci -profile extra
expected one command and one flag
$ echo $?
1
```

The record is built by the runtime and is read-only. Because there is no
field assignment and parameters are read-only bindings, a program cannot
corrupt its own arguments after parsing; [assignment and read-only
parameters](/language/03-names-and-scope/03-assignment/)
covers the binding rule.
