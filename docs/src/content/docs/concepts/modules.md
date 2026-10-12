---
title: Modules
description: Namespaces, visibility, names and scope, imports, and the entry.
---

As a program grows, you group its code into namespaces with `mod` and load it
from other files with `import`. This chapter covers namespaces, visibility,
names and scope, imports, top-level values, and the entry.

## Namespaces

A `mod a::b { ... };` block declares an **inline namespace**. Every declaration
inside belongs to `a::b`:

```kiru
mod tasks::build {
  use std;
  use std::text;
  pub fn run(line: txt) { std::command(line).run(); };
};

fn main() {
  tasks::build::run("echo building");
};
```

`tasks::build::run` is reached by that path; `run` is `pub`, so another
namespace may name it. A file may hold several `mod` blocks, and **reopening a
path merges** into the same namespace, so one namespace can span blocks and
files:

```kiru
use std;
use std::text;

mod tasks {
  pub fn clean() { std::print("clean"); };
};

mod tasks {
  pub fn build() { clean(); };
};

fn main() {
  tasks::build();
};
```

Paths nest: `tasks::build` is inside `tasks`. A bare name is looked up only in
the namespace that holds it and the namespaces that namespace `use`s; there is
no climb to a parent namespace. `build` can call `clean` because both
declarations live in `tasks`.

`std` is reserved, and a program cannot declare anything under it — a
`mod std` block is rejected:

```console
$ kc main.kiru
main.kiru:2:10: error: `std` is reserved and cannot be declared
  pub fn f() {};
         ^
```

Its functions are reached through the `std::` prefix.

## Use and visibility

Every declaration is **private** to its own namespace unless it is marked
`pub`. The rule covers a type's members as well as a namespace's own
declarations: a **field** is a declaration, and so is an **enum variant**. A
field takes `pub` before its name (`pub host: txt`), and a variant takes it
before its name too (`pub Ready`). A qualified path names a namespace directly,
and the target must be `pub`:

```kiru
use std;
use std::text;

mod tasks::build {
  use std;
  use std::text;
  pub fn run(line: txt) { std::command(line).run(); };   # reached by path
  fn helper() { std::print("private"); };                    # `tasks::build` only
};

fn main() {
  tasks::build::run("echo running");   # `run` is pub
};
```

A field and a variant follow the same rule. A reader outside the declaring
namespace may read a `pub` field and construct a `pub` variant, but a private
field is reached through a function or method the namespace provides, and a
private variant only inside its own namespace:

```kiru
use std;
use std::text;

mod config {
  use std;
  use std::text;
  pub rec Server {
    pub host: txt,   # any namespace may read `s.host`
    port: int,       # `config` only; `s.port()` reads it
  };
  pub fn server(host: txt, port: int) -> Server {
    return { host = host, port = port };   # inside `config`, so `port` is legal
  };
  pub fn port(self: Server) -> int { return self.port; };

  pub enum State {
    pub Ready,
    pub Starting,
  };
};

use config;

fn main() {
  let s: config::Server = config::server("localhost", 8080);
  std::print("@(s.host) @(s.port())");
  let state: config::State = config::State.Ready;
  std::print(match state {
    config::State.Ready => "ready";
    config::State.Starting => "starting";
  });
};
```

Reaching a private member from outside is an error that names the declaring
namespace: `` `Server.port` is private to `config`; mark it `pub` to reach it ``.
The same error appears for a private field in a `{ … }` literal, a private field
read or assigned, and a private variant in a construction or a pattern.

A `use a::b;` written beside the declarations of a namespace brings the public
names of `a::b` into that namespace, so they can be written bare:

```kiru
use std;
use std::text;

mod tasks::build {
  use std;
  use std::text;
  pub fn run(line: txt) { std::command(line).run(); };
};

mod app {
  use tasks::build;
  pub fn go() { run("echo built"); };   # `run`, brought in by `use`
};

fn main() {
  app::go();
};
```

A `use` path may name a namespace (`use tasks::build;`) or a single declaration
(`use tasks::build::run;`, which brings only `run`). A `use` at the top level
adds its names to the root namespace.

A method is a declaration like any other, reached through the same `use`/`pub`
mechanism: the declaring namespace must be in scope for the call to resolve.
[Methods](/concepts/methods/) covers dispatch and reachability.

The compiler's built-in functions are `pub` by construction, and the standard
library declares every type it owns. The runtime types are bodyless `type`
declarations in `std` categories, so a program reaches them the way it reaches
`Option`: `txt`, `Vec<T>`, `bool`, and `int` are written after `use std::text;`,
`use std::vec;`, and `use std;`. A library type is an ordinary declaration:
`Option` is an `enum` in `std`, and `Map<K, V>` is a `rec` in `std::map`, each
written by a qualified path or brought in with its `use`.

## Rooted names

A leading `::` starts at the root namespace instead of the current one, so a
root name that a local shadows stays reachable. The root declaration must be
`pub`, because a rooted path names a namespace directly and reaches only `pub`
declarations:

```kiru
use std;
use std::text;
pub let name: txt = "root";

fn show() {
  let name: txt = "local";
  std::print(name);      # local
  std::print(::name);    # root
};

fn main() { show(); };
```

The same prefix works for a call: `::build()` reaches the root `build`, which
must be `pub`.

## Declare before use

Names are read top-down. A name must be declared before the point that uses it;
a forward reference is an error:

```console
$ kc main.kiru
main.kiru:2:18: error: `banner` is declared after this point
let title: txt = banner("kiru");
                 ^^^^^^
```

A function body sees, in order of closeness, its parameters, the bindings it
declares itself, and the declarations above the function. It does not see
another body's locals.

A function cannot call itself, and a value cannot reference itself. Since every
name is declared before its uses, a program has no recursion.

## Unique names

A namespace keeps one pool of names, and a value and a function share it: a name
is declared once, whether it names a value or a function. Reusing a name — even
once as a value and once as a function — is an error:

```kiru
use std::text;
let thing: txt = "value thing";
fn thing() -> txt { return "function thing"; };
fn main() {};
```

```console
$ kc main.kiru
main.kiru:3:4: error: `thing` is declared more than once in this namespace
fn thing() -> txt { return "function thing"; };
   ^^^^^
```

The one exception is a method family: methods whose first parameter has a
receiver type head may share a name, once per head, because the receiver's type
head selects between them. Two methods for the same head are still an error.
[Methods](/concepts/methods/) covers the rule.

Within one function body, a local cannot duplicate a name already local in that
body, but it may shadow a top-level name.

## Scopes

Each function body is a scope. Each match arm and each loop body is its own
scope too: a name declared in one arm is not visible in another, and a name
declared in a loop body is not visible after the loop.

Nested blocks still share the enclosing function's bindings for assignment, so
an arm or a loop body can reassign a binding declared outside it.
[Match](/concepts/match/) and [Vecs](/concepts/vecs/) show
the shape.

## Imports

`import "path";` loads another file. The path is relative to the importing file
or absolute, and is always a fixed string; it is never expanded, so an import
cannot use a runtime value such as `$HOME`.

An import is an **item**: the imported file's items are spliced **at that
position**, so the order of the imports is the declaration order. An import is
**global-level only** — it is never written inside a `mod`.

```kiru
import "tasks.kiru";

fn main() {
  tasks::build::run("echo imported");
};
```

Here `tasks.kiru` declares `mod tasks::build { pub fn run(...) ... };`. An import
does not bring names into scope — it makes the file's namespaces part of the
program, and a name is still reached by a path or a `use`.

A file is loaded **once**: importing the same file from two places, or importing
in a cycle, is an error.

## Top-level values

A `let` at the top level is a top-level value. Top-level values are computed
once, when the program starts, in order, before `main`:

```kiru
use std;
use std::text;
rec Project { name: txt, dir: txt };
let home: txt = "/home/dev";

let backend: Project = {
  name = "backend",
  dir = "@(home)/projects/backend",
};

fn main() {
  std::print(backend.dir);
};
```

A later reference reads the stored value; the initializer does not run again. A
top-level value can call a function declared above it, and cannot be assigned.

## The entry

`main` is the **root namespace's** function — declared at the top level of some
file, not inside a `mod`. A `main` inside a `mod` is an ordinary function, so
the compiler requires a root `main` or the program does not build:

```console
$ kc main.kiru
main.kiru:1:1: error: the program has no root `main` function
mod app {
^
```

[A First Program](/getting-started/a-first-program/) covers `main`'s parameter
and return.
