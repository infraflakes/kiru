---
title: Signals and Shutdown
description: Ctrl+C, SIGTERM, SIGHUP, and exit code 130.
---

A program can be stopped from outside, by Ctrl+C or a supervisor. This chapter
covers what Kiru does when that happens.

Ctrl+C, SIGTERM, and SIGHUP terminate the program and exit `130`. The runtime
notes which signal arrived, marks the program cancelled, and stops every
running process group with the protocol in
[Process Groups](/commands/process-groups/), then exits `130`.

`130` is `128 + 2`, the shell convention for a process ended by SIGINT, so a
script or CI step can tell "the program failed" (exit `1`) from "a person or
supervisor stopped it" (`130`).

Every command runs in its own process group, so a signal sent to the terminal
reaches the Kiru process but not its children directly. The runtime forwards
the stop to each registered group, so a `cargo build` started by a Kiru program
does not survive Ctrl+C. [Process Groups](/commands/process-groups/) covers the
grouping.

The signal handlers are installed before the program starts, ahead of the
top-level values and `main`.
