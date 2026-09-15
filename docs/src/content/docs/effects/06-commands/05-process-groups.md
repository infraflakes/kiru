---
title: Process Groups
description: How commands are grouped and stopped.
---

Every command runs in its own process group; on Linux it also receives a
parent-death signal. A process group is a set of processes that can be
signalled together, so stopping a command stops everything it spawned with
one signal:

```text
kiru
|-- group 101: sh -c "cargo build"    (stdout shown)
|     `-- rustc
`-- group 102: sh -c "bun run build"  (a second command)
      `-- node
```

On Linux, each child is marked to receive a signal if its parent dies. If
Kiru is killed without a chance to clean up, its commands do not linger.

## Stopping

Stopping a command sends SIGTERM to the group, then SIGKILL after two
seconds. The two-second grace period is fixed; there is no configuration.

```text
SIGTERM  ->  wait 2s  ->  SIGKILL
```
