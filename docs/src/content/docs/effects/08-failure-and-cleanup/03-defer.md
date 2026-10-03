---
title: Defer
description: Cleanup at body exit, LIFO order, and behavior during unwind.
---

`defer { ... };` registers cleanup that runs when the enclosing body exits:
on the normal path, on a return, and when a panic unwinds the body.

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn release() {
  std::command({}, "mkdir -p /tmp/kiru-release");

  defer {
    std::command({}, "rm -rf /tmp/kiru-release");
  };

  std::command({}, "true");
};
```

The first command creates the staging directory at a known path, and the
defer registers its removal, so the directory is removed whether the body
succeeds or fails.

## LIFO Order

Defers run in reverse declaration order:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  defer { std::print("first registered, last run"); };
  defer { std::print("second registered, first run"); };
};
```

```console
$ ./app
second registered, first run
first registered, last run
```

The second registration runs first, which matches resource acquisition: the
last thing acquired is the first thing released.

## Sharing the Body's Bindings

A defer body has its own declaration scope. A name it declares is not
visible after the defer, and it may not shadow a visible name. It still
assigns the enclosing body's bindings, and it reads them as they are when it
runs:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  txt name = "before";
  defer { std::print("defer sees " + name); };
  name = "after";
};
```

```console
$ ./app
defer sees after
```

The defer runs after the assignment, so it reads `after`. A name declared
inside the defer belongs to the defer alone:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  defer {
    txt temp = "x";
    std::print(temp);
  };
  std::print(temp);
};
```

```console
$ kc main.kiru
main.kiru:6:14: error: unknown name `temp`
  std::print(temp);
             ^^^^
```

## Return Inside a Defer

`return` is allowed inside a defer. It ends that defer body, and its value is
discarded; the enclosing body still returns normally. Later defers still run:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn main() {
  defer {
    std::print("cleanup");
    return();
    std::print("unreachable");
  };
  std::print("body");
};
```

```console
$ ./app
body
cleanup
```

## Defers and Threads

The runtime joins every remaining thread before the program exits. A
function that starts a thread and needs its work finished before its own
return still runs `wait;`, which joins the asyncs the calling thread spawned.
A function started with `async` may register its own defers; they run when its
body ends.

## During Unwind

When a panic unwinds a body, that body's defers run before the panic
continues outward. Each body runs its defers as the panic passes through,
so every registered cleanup runs exactly once, innermost first. A panic
inside a defer is reported, and the remaining defers still run; the process
exits nonzero. Because defers run on the panic path, they see the bindings
as the body left them.
