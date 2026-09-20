---
title: Failure and Cleanup
description: Panic, signals, defers, and unwind order.
---

This chapter covers how a run ends and how it cleans up. It describes
`panic` and `eprint`, the signals that take the same path, the defers that
run on the way out, and the order they run in during an unwind.
