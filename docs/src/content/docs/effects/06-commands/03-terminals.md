---
title: Terminals
description: stream, out, and code, and the rules for combining them.
---

`.out` and `.code` are the only terminals. Each runs the chain and
returns text. `.stream` is a builder: it keeps the chain going and only
marks that stdout shows when the command runs.

| Method | Accepts | Returns | Meaning |
| --- | --- | --- | --- |
| `.stream()` | command | command | Marks stdout to show live. Alone as a statement, the chain runs and binds nothing. stderr is always forwarded. |
| `.out()` | command | text | Runs and binds stdout with trailing newlines trimmed. Without `.stream()`, output is not shown; stderr is always forwarded. |
| `.code()` | command | text | Runs and binds the exit code as text. Without `.stream()`, stdout is not shown; stderr is always forwarded. |

## Streaming

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("cargo build").stream();
};
```

`.stream` marks the chain's stdout to be shown as it is produced. The
command runs when the chain is evaluated, and the statement binds nothing.

## Capturing

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  txt version = std::command("printf '1.2.3\n'").out();
  std::print(version);
};
```

`.out` runs the command quietly and binds its stdout as text, with
trailing newlines trimmed, so the program prints `1.2.3`. stderr is still
forwarded. A terminal result is data, suited to a version, a path, or a
hash.

## Reading the Exit Code

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  txt code = std::command("true").code();
  std::print("code " + code);
};
```

```console
$ ./app
code 0
```

`.code` binds the exit code as text. Without `.stream`, stdout is not
shown. stderr is always forwarded, so errors remain visible.

## Combining

`.stream` combined with `.out` shows and captures at once:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  txt output = std::command("echo shown").stream().out();
  std::print("captured [" + output + "]");
};
```

```console
$ ./app
shown
captured [shown]
```

The command's stdout is written to the terminal and collected into
`output`. `.out` and `.code` cannot be combined, because each one ends
the chain with a different text:

```console
$ kc main.kiru
main.kiru:2:11: error: `.out()` and `.code()` cannot be combined
  txt x = std::command("ls").out().code();
          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

After a terminal the value is text, so a further method needs a command:

```console
$ kc main.kiru
main.kiru:2:11: error: expected command, found text
  txt x = std::command("ls").code().stream();
          ^^^^^^^^^^^^^^^^^^^^^^^^^
```
