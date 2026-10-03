---
title: Threads
description: Starting functions on their own threads and joining them.
---

This chapter covers running functions concurrently. It describes starting a
call on its own thread with the `async` keyword, joining the asyncs the
calling thread spawned with `wait`, why only the entry thread owns the
terminal, and what happens when a thread panics.
