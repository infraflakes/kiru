---
title: Process Groups
description: How commands are grouped and how a stop reaches them.
---

Kiru runs every command in its own process group, so it can stop a command and
everything it started with one signal. This chapter covers the grouping and the
stop protocol.

A process group is a set of processes that can be signalled together, so
stopping a command stops everything it spawned with one signal. On Linux, each
child is also marked to receive a parent-death signal, so commands do not
linger if Kiru is killed without a chance to clean up:

```text
kiru
|-- group 101: sh -c "cargo build"    (stdout shown)
|     `-- rustc
`-- group 102: sh -c "bun run build"  (a second command)
      `-- node
```

## Stopping

Stopping a command sends `SIGTERM` to the group, then `SIGKILL` after two
seconds. The two-second grace period is fixed:

```text
SIGTERM  ->  wait 2s  ->  SIGKILL
```

This is the one stop protocol. A stop runs when a `panic;` or a runtime error
fails the program, and when a signal arrives.
[Panic and Failure](/failure/panic/) covers failure, and
[Signals and Shutdown](/failure/signals/) covers Ctrl+C.
