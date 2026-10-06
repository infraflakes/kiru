---
title: Process Groups
description: How commands are grouped and stopped.
---

Kiru runs every command in its own process group, so it can stop a command and
everything it started with one signal. In this section we'll see how the groups
work.

Every command runs in its own process group; on Linux it also receives a
parent-death signal. A process group is a set of processes that can be
signalled together, so stopping a command stops everything it spawned with one
signal:

```text
kiru
|-- group 101: sh -c "cargo build"    (stdout shown)
|     `-- rustc
`-- group 102: sh -c "bun run build"  (a second command)
      `-- node
```

On Linux, each child is marked to receive a signal if its parent dies, so
commands do not linger if Kiru is killed without a chance to clean up.

## Stopping

Stopping a command sends SIGTERM to the group, then SIGKILL after two seconds.
The two-second grace period is fixed.

```text
SIGTERM  ->  wait 2s  ->  SIGKILL
```

[Panic and Failure](/failure/panic/) and [Signals and
Shutdown](/failure/signals/) cover when this happens.
