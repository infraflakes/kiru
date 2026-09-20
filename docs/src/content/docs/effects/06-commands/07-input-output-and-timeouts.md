---
title: Input, Output, and Timeouts
description: stdin, stderr, decoding, and how a timeout stops a group.
---

The kernel defines how every command is run.

## Input

stdin is inherited. A command that reads from the terminal reads the same
terminal the program was started from, so interactive commands work under
`.stream`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  std::command("read -p 'name: ' who; echo $who").stream();
};
```

The shell prints `name: ` and waits for a line; `$who` then echoes it. The
same terminal is used because stdin is inherited rather than captured.

## Output

stderr is always forwarded. stdout depends on the chain: shown under
`.stream`, captured by `.out`, discarded by `.code`, as [streaming
and capturing](/effects/06-commands/04-streaming-and-capturing/) describes.
Output is decoded lossily, leading whitespace is preserved, and captured
stdout has trailing newlines trimmed.

## Timeouts

A timeout is whole seconds. On expiry the process group receives SIGTERM,
then SIGKILL after two seconds, and the command's code becomes `"124"`:

<span class="filename">Filename: src/main.kiru</span>

```kiru
txt code = std::command("sleep 60").timeout("1").code();
```

```console
$ # code is "124"
```

`124` is the conventional timeout code, and it is returned whether the
command died on SIGTERM or had to be killed. A program can tell a timeout
from an ordinary failure by comparing the code.

## The Waiting Loop

While a command runs, the runtime polls it and watches two things: whether
the run was cancelled, and whether the deadline has passed. The first
condition stops the group and lets the panic path take over; the second
stops the group and returns `124`. Polling means a cancelled run is noticed
promptly even when the command is quiet.

## Summary

```text
process group   every command, always
parent death    on Linux
stop            SIGTERM, 2s, SIGKILL
timeout         SIGTERM, 2s, SIGKILL, code "124"
stdin           inherited
stderr          forwarded
stdout          chain-dependent
decode          lossy UTF-8
capture         trailing newlines trimmed
```
