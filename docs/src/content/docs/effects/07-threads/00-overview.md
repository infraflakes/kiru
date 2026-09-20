---
title: Threads
description: Starting functions on their own threads and joining them.
---

This chapter covers running functions concurrently. It describes starting a
call on its own thread with `std::async`, joining the asyncs the calling
thread spawned with `std::wait`, and what happens when a thread panics.
