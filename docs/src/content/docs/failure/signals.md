---
title: Signals and Shutdown
description: Ctrl+C, SIGTERM, SIGHUP, and exit code 130.
---

Sometimes a program is stopped from outside, by Ctrl+C or a supervisor. In this
section we'll see what Kiru does when that happens.

Ctrl+C, SIGTERM, and SIGHUP terminate the program and exit `130`. Unlike a
panic, they stop every running process group:

1. note which signal arrived;
2. mark the program cancelled;
3. send SIGTERM to every running process group;
4. wait two seconds, then SIGKILL;
5. exit `130`.

`130` is `128 + 2`, the shell convention for a process ended by SIGINT, so a
script or CI step can tell "the program failed" (exit `1`) from "a person or
supervisor stopped it" (`130`).

Every command runs in its own process group, so a signal sent to the terminal
reaches the Kiru process but not its children directly. The runtime forwards
the stop to each registered group, so a `cargo build` started by a Kiru program
does not survive Ctrl+C. [Process Groups](/commands/process-groups/) covers
the grouping.

The signal handlers are installed before the program starts, ahead of the
top-level values and `main`.
