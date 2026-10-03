---
title: Commands
description: std::run, the std::command spec, streaming levels, exit codes, and process groups.
---

This chapter covers running external commands. `std::run` runs one shell line
and returns its exit code; the streams are rendered live, never captured.
`std::command` is the record-driven abstraction over it: it takes a spec whose
uppercase entries enable features and returns the same exit code.
