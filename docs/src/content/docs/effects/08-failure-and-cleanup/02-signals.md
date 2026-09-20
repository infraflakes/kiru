---
title: Signals
description: Ctrl+C, SIGTERM, SIGHUP, and exit code 130.
---

Ctrl+C, SIGTERM, and SIGHUP stop the run, with two differences from a
panic: they stop every running process group, and the exit code is `130`.

```console
$ ./app build
^C
$ echo $?
130
```

The runtime:

1. notes which signal arrived,
2. marks the run cancelled,
3. sends SIGTERM to every running process group,
4. waits two seconds, then SIGKILL,
5. runs pending defers,
6. exits `130`.

## Exit Code 130

`130` is `128 + 2`, the shell convention for a process ended by SIGINT. A
script or CI step can distinguish "the program decided to fail" (nonzero)
from "a person or supervisor stopped it" (`130`).

## Children Are Stopped

Every command runs in its own process group, so a signal sent to the
terminal reaches the Kiru process but not its children directly. The
runtime forwards the stop to each registered group, so a `cargo build`
started by a Kiru program does not survive Ctrl+C.

## Defers Still Run

Signals run defers, like any unwind. A release interrupted halfway still
removes its staging directory:

```console
$ ./app release
staging in /tmp/tmp.vPcHLI4ozr
^C
$ test -e /tmp/tmp.vPcHLI4ozr || echo cleaned
cleaned
$ echo $?
130
```

## Installing the Handlers

The handlers are installed before the program starts, ahead of the
top-level values and `main`. Before that, the compiler's own process has
default signal behavior, which is what you want: Ctrl+C during compilation
kills the compiler.

:::caution
A defer that runs on a signal runs while the run is being cancelled. Keep
cleanup commands short and non-interactive.
:::
