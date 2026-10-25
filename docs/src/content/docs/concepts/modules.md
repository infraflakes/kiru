---
title: Namespaces
description: Directories, absolute paths, use, visibility, names, and the entry.
---

As a program grows, you group its code by directory. A directory is a
**namespace**, and a file in it declares into that namespace. There is no `mod`
and no `import`: the program is the tree of files under the manifest's
`project.root`, and each file's directory decides where its names live.

## A namespace is a directory

A file belongs to its directory's namespace, and its filename is arbitrary.
Nesting nests:

```text
project/
  kiru.toml            # names this tree
  main.kiru            # root
  build/
    plan.kiru          # root::build
    run.kiru           # root::build
  tools/
    deploy/
      release.kiru     # root::tools::deploy
```

Every file under `build/` declares into `root::build`, whatever it is named;
`plan.kiru` and `run.kiru` are two files of one namespace, and a name declared
in one is a name of the other. A file may be renamed, split, or merged freely,
because only the directory names the namespace.

The standard library is the `std` tree, so `std/dynamic/vec.kiru` declares into
`std::dynamic`. A program's own tree is the `root` tree: every file under the
manifest's `project.root` is the program, and that directory itself is `root`.

## Absolute paths

Every absolute path resolves on its own, with no `use`. A path starts at a tree
name (`std` or `root`) and walks down:

```kiru
use std;

fn main() {
  let xs: std::dynamic::Vec<int> = std::dynamic::Vec.new();
  xs = std::dynamic::push(xs, 1);
  std::print("@(std::dynamic::len(xs))");
};
```

`std::dynamic::Vec` is the type; `std::dynamic::Vec.new()` is its associated
function; `std::dynamic::push` is a native in that namespace. A declaration
reached across a namespace must be `pub`; `std::` holds only public names.

The standard library follows one naming rule: it writes full paths and uses no
`use`, so every stdlib file reads as its absolute path (`std::dynamic::push`,
`std::text::strlen`). A native is named for its syscall or its kind: one that
touches the operating system lives in `std::syscalls` under its syscall or C
library name (`posix_spawn`, `mkstemp`, `waitpid`), and a pure one stays with its
kind, the container's in `std::dynamic` and the text kernel's in `std::text`.

## use shortens a path

A `use` is pure path-shortening. It may stand anywhere among a file's
top-level items, and belongs to the namespace, not to the file. What the path
ends at decides what it shortens, and there are three endings:

- a **namespace** gives the qualifier its last segment spells:

  ```kiru
  use std;
  use std::dynamic;

  fn main() {
    let xs: dynamic::Vec<int> = dynamic::Vec.new();
    std::print("@(dynamic::len(xs))");
  };
  ```

- a **type** gives that type bare, and brings its bound functions with it:

  ```kiru
  use std;
  use std::dynamic::Vec;

  fn main() {
    let xs: Vec<txt> = Vec.new();
    xs = xs.push("b");
    xs = xs.push("a");
    std::print(xs.sort().at(0));
  };
  ```

- a **plain function** gives that one name bare:

  ```kiru
  use std;
  use std::process::quote;

  fn main() {
    std::print(quote("a b"));
  };
  ```

One level of braces shortens several names under one prefix:

```kiru
use std;
use std::dynamic::{HashMap, Vec};

fn main() {
  let m: HashMap<txt, int> = HashMap.new();
  let xs: Vec<int> = Vec.new();
  std::print("@(m.len()) @(xs.len())");
};
```

There is no alias and no `as`. A `use` does not reach a sub-namespace and does
not chain: `use std::dynamic;` gives `dynamic::Vec`, not `dynamic::Vec.new`
without the `Vec`, and it never reaches `std::text`. A `use` also never reaches
a module value or a bound function — a bound function is reached through its
type ([Methods](/concepts/methods/) covers that).

## Resolution is order-free

Every declaration is collected into its namespace first, and only then are
references resolved. A name may therefore be declared after the point that
names it, and a function may call itself:

```kiru
use std;

fn main() {
  std::print(banner("kiru"));
};

fn banner(name: txt) -> txt {
  return "=== @(name) ===";
};
```

A forward reference and a cycle inside one namespace are both legal. The order
in a file, and the order of the files in a directory, does not matter.

## Visibility

Every declaration is **private** to its own namespace unless it is marked
`pub`. The rule covers a type's members as well as a namespace's own
declarations: a **field** is a declaration, and so is an **enum variant**. A
field takes `pub` before its name (`pub host: txt`), and a variant takes it
before its name too (`pub Ready`).

A qualified path reaches only `pub` declarations; a bare name reaches its own
namespace whatever the visibility. Nothing climbs to a parent: `root::build`
does not see `root`'s names without a `use` or an absolute path.

```kiru
use std;

pub rec Server {
  pub host: txt,
  port: int,
};

fn main() {
  let s: Server = { host = "localhost", port = 8080 };
  std::print(s.host);
};
```

Here `host` is `pub` and `port` is private. Both are reachable from `main`,
because `main` is in the same namespace that declares `Server`; another
namespace would reach `host` by its path and would reach `port` only through a
method.

Reaching a private member from outside is an error that names the declaring
namespace. A bound function follows the same rule: it is reachable across a
namespace only when the function is `pub` and its type is `pub`, and it is
never gated by a `use` ([Methods](/concepts/methods/) covers it).

## Unique names

A namespace keeps one pool of names, and a name holds exactly one declaration.
Reusing a name — whether it names a value, a function, or a type — is an error:

```console
$ kc
main.kiru:3:4: error: `thing` is declared more than once in this namespace
fn thing() -> txt { return "function thing"; };
   ^^^^^
```

The one exception is a bound function: two functions bound to different types
may share a name, because the receiver's type head selects between them.
`Vec.len` and `HashMap.len` share `len` in `std::dynamic`. Two functions bound to
the same type, or two plain declarations of one name, are still an error.
[Methods](/concepts/methods/) covers the rule.

Within one function body, a local cannot duplicate a name already local in that
body, but it may shadow a top-level name.

## Scopes

Each function body is a scope. Each match arm and each loop body is its own
scope too: a name declared in one arm is not visible in another, and a name
declared in a loop body is not visible after the loop.

Nested blocks still share the enclosing function's bindings for assignment, so
an arm or a loop body can reassign a binding declared outside it.
[Match](/concepts/match/) and [Vecs](/concepts/vecs/) show the shape.

## Module values

A `let` at the top level is a **module value**. Module values are computed once,
when the program starts, before `main`, and a later reference reads the stored
value; the initializer does not run again.

```kiru
use std;

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

A module value is private to its namespace: it is evaluated at startup, so it
has no cross-namespace form, and `pub` on a `let` is rejected:

```console
$ kc
main.kiru:2:9: error: a module value cannot be `pub`; it is private to its namespace
pub let name: txt = "root";
        ^^^^
```

Within a namespace, module values are ordered by their initializers — a value
is evaluated before the values that read it — and a cycle among them is an
error. A module value cannot be assigned.

## The entry

`main` is the **root namespace's** function: declared at the top level of a
file in the entry's directory, not in a subdirectory. `main` takes the words
and returns the exit code:

```kiru
use std;
use std::dynamic::Vec;

fn main(words: Vec<txt>) -> int {
  match words.len() {
    0 => { std::print("no arguments"); };
    _ => { std::print("first: @(words.at(0))"); };
  };
  return 0;
};
```

The parameter must be `Vec<txt>`, the words after the program name, and the
return type must be `int` or absent. [A First
Program](/getting-started/a-first-program/) covers the entry in full.
