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

## LIFO Order

Defers run in reverse declaration order, and there is no priority, condition,
or way to skip one, so the last thing acquired is the first thing released:

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

A name declared inside the defer belongs to the defer alone:

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

A function started with `async` registers its own defers, and they run when
its body ends. [Threads](/effects/07-threads/01-threads/) covers waiting and
joining.

## During Unwind

When a panic unwinds a body, that body's defers run before the panic
continues outward. Each enclosing body runs its defers as the panic passes
through, so every registered cleanup runs exactly once, innermost body first.
A panic inside a defer is reported, and the remaining defers still run; the
process exits nonzero. Because defers run on the panic path, they see the
bindings as the body left them.
