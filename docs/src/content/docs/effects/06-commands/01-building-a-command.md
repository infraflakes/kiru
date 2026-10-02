---
title: Running a command
description: std::run and the out and code fields.
---

`std::run(<text>)` runs one command line through the shell. It streams stdout
live, captures it, and returns a record with two text fields:

| field | meaning |
| --- | --- |
| `out` | captured stdout, trailing newlines trimmed |
| `code` | the exit code as text |

A record from a call is read through its fields, not bound whole:

    txt tag = std::run("git describe --always").out;
    txt code = std::run("make").code;

`std::run` is the primitive. `std::command` is the record-driven abstraction
over it, and most code calls `std::command` instead.
