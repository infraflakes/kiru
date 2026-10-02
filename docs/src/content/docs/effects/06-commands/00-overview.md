---
title: Commands
description: std::run, the std::command spec, modes, exit codes, and process groups.
---

This chapter covers running external commands. `std::run` runs one shell line
and returns `{ out, code }`. `std::command` is the record-driven abstraction
over it: it takes a spec whose uppercase entries enable features and returns
the text the spec's `Mode` asks for.
