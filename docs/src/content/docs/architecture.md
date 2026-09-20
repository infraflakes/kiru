---
title: Architecture
description: One arena for execution and display, one view, one process supervisor.
---

The compiled program is the center of kiru: one structure serves execution, the
live TUI, and the final dump. There is no second model to keep in sync.

## The Program Is an Arena

`kiru compile` lowers the source into a `Program`: a list of nodes plus each
run's roots.

- A node is one statement or block: `log`, `exec`, `cd`, `env`, `async`,
  `switch`, an arm, or a project context, with its children in the arena.
- Parameters are substituted at compile time; functions are inlined at each
  call site.
- Variables are stored once in the program's variable table. A reference
  carries only the variable's identity, so a large value never duplicates in
  the compiled file.

## The Executor Writes State Directly

Running a program walks the arena and writes each node's runtime state: status,
output lines, and the values that only exist at run time, such as a resolved
label or project name. There are no events and no second list of rows; the
display state arena is indexed by the same node identities.

- **Async** bodies run in scoped threads and are joined when the enclosing body
  ends, so nothing outlives its parent.
- **Failure** marks the run lost, kills every live process group, and cancels
  pending sibling rows; the failing row shows its own error once.
- **Switch** arms that are not taken end terminal as skipped and are pruned
  from the view.

## One View

The live TUI and the final dump are one walk over the program and its states.
The walk computes the tree prefixes, prunes skipped subtrees, and prints
resolved values. When stdout is not a terminal there is no live view at all,
only the dump.

## Process Supervision

Every command is its own process-group leader, so a stop reaches wrappers and
grandchildren. Cancelling sends SIGTERM first, then SIGKILL after a short
grace. A failing task stops its siblings' groups the same way. On Linux a
parent-death signal ties children to kiru's lifetime, and SIGINT, SIGTERM, and
SIGHUP all run the same cancel path, so nothing is left behind.

## The Compiled Form

The compiled program is a RON file with the shape above: nodes, run roots, and
the variable table. It contains no shell results, so it stays portable.
Variables that hold commands compute at execution, once, and are reused.
