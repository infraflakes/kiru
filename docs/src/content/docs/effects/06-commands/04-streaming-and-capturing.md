---
title: Streaming and Capturing
description: Which file descriptors are inherited, piped, or discarded.
---

The chain decides which file descriptors the child receives. stderr is
always forwarded, whatever the chain does with stdout:

| Chain | stdout | stderr |
| --- | --- | --- |
| `...stream()` | inherited | inherited |
| `...out()` | piped and bound | inherited |
| `...code()` | discarded | inherited |
| `...stream().out()` | piped, shown, and bound | inherited |
| `...stream().code()` | inherited | inherited |

The terminal rules are described in
[terminals](/effects/06-commands/03-terminals/); this page covers the
descriptor behavior and the captured text.

## Capture Details

Captured stdout is decoded lossily and has trailing newlines trimmed. Text
is otherwise exact, including leading whitespace:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  txt out = std::command("printf '  indented\n\n'").out();
  std::print("[" + out + "]");
};
```

```console
$ ./app
[  indented]
```

The command prints two leading spaces and two trailing newlines; both
newlines are trimmed and the spaces are kept. The output is a byte stream
from a pipe, so invalid UTF-8 is replaced rather than failing the command.
Text is the only data the program manipulates, and lossy decoding keeps a
misbehaving command from breaking the run.

:::note
`.stream` marks the chain's stdout to be shown, so the child receives the
user's stdout: colors, progress bars, and interactivity work as they do in
a shell.
:::
