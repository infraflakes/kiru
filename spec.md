# Kiru language spec

## The basics

Kiru has three scalars: `txt`, `bool`, and `int`.

The keywords and forms are:

`fn`, `rec`, `enum`, `impl`, `let`, `use`, `pub`, `match`, `return`, `panic`, `for`, `break`, `async`, `wait`, `true`, `false`.

`txt`, `bool`, and `int` are scalar names, not keywords. `self` is not a keyword either. It is just a parameter name people usually use for a method's receiver.

Everything else is a declaration in the library. A built-in function is a native: it is used like a function, but it has no body and no source file.

## Two name pools

At namespace level, Kiru has two separate name pools. They do not collide with each other. A type's methods and associates are a separate scope inside the type.

### Type pool

This holds names that mean a type:

- `rec` declarations
- `enum` declarations
- the scalar names `txt`, `bool`, `int`
- declared type names from the library, such as `Vec`, `Map`, `Set`, and `Option`

A type name is unique inside its namespace. A type can share a spelling with a value, because they are different pools.

### Value pool

This holds names that mean a value:

- named functions
- module values (a top-level `let`)
- locals and parameters
- natives

A value name is unique inside its namespace. A function and a module value share this pool, so they cannot use the same spelling.

### Methods and associates

Methods and associates are not namespace-level names. They belong to their type.

- Every function inside one `impl T` shares one impl scope.
- A method is a function whose first parameter has type `T`.
- An associate is a function whose first parameter is not `T`, including a function with no parameters. The remaining parameters do not decide: a function with four parameters of type `T` is still an associate if its first parameter is not `T`.
- A type owns its methods and associates. A name is unique inside that type's impl scope; methods and associates share that one scope. The same name can exist on a different type.
- A method or associate never collides with a namespace-level function or value, even when the spelling is the same.
- A named type's methods and associates must be declared in the same namespace as the type. An `impl` block only looks for its type in its own namespace. It does not reach a type through a `use` or an absolute path from another namespace. This rule is for source `impl` blocks; a native is provided by the runtime and may be bound to a type from its own namespace.

Example:

```
rec Service { name: txt };

# A type and a function may share a spelling.
fn Service() -> Service {
  return { name = "api" };
};

# This block must be in the same namespace as Service.
impl Service {
  pub fn line(self: Service) -> txt {
    return self.name;
  };
};
```

`impl` only has one job: it puts functions under a named type. There are no traits, no interfaces, and no `impl Trait for Type`.

## Files, namespaces, and the project

A compilation is one project, named by `kiru.toml`.

- `project.root` is the program tree.
- Every `.kiru` file under that root joins the program.
- `project.output` is where the binary is written.
- The tree's directories are the namespaces, with `root` as the first segment.
- A file belongs to the namespace of its directory. The file name does not matter.
- There is no `mod` and no `import`.

Resolution is order-free. Every declaration in the project is collected first, then references resolve. A forward reference is fine. A cycle inside one namespace is fine.

`main` must be declared in the root namespace. A `main` in a sub-namespace is an ordinary function. See the `main` section below.

## Paths and `use`

Every public function, type, field, variant, and bound function can be named from anywhere by its absolute path:

```
root::kiru::usage()
std::dynamic::Vec.new()
std::Option.some(1)
```

`use` shortens a path. It does not change visibility.

```
use std::dynamic;          # dynamic::Vec<T>
use std::dynamic::Vec;     # Vec<T>
use std::dynamic::{Map, Vec};
```

Things to remember:

- A `use` may end at a namespace, or at a name that may denote a type and/or a plain function.
- A `use` of a name brings every type and plain function that name denotes. The type pool and value pool are separate, so a type and a function can share a spelling and one `use` brings both. Visibility is checked where the name is read, not by the `use` itself.
- A `use` may not name a module value, a method, or an associate function. Those are reached through their owner.
- A type-`use` brings the type's bound functions with it.
- There is no `as`.
- A `use` belongs to the namespace, not to one file.

For example:

```
# root/api/mod.kiru
pub rec Item { name: txt };
pub fn Item() -> Item { return { name = "api" }; };

# root/main.kiru
use root::api::Item;      # brings both the type Item and the function Item()
let item: Item = Item();
```

Visibility is a separate gate from paths:

- A named declaration can be `pub` or private.
- A field or variant can be `pub` or private.
- Module values are never `pub`.
- A method or associate is reachable across namespaces only when both the type and the function are `pub`.

## Declarations

The declaration forms are:

```
fn name(parameters) -> ReturnType { ... };
fn name(parameters) { ... };

rec Name<T> {
  pub field: Type,
  other: Type,
};

enum Name<T> {
  pub variant(Type),
  other,
};

impl Type {
  pub fn method(self: Type) -> Type { ... };
  pub fn associate() -> Type { ... };
};

let name: Type = value;

use path;
use path::{name, other};
```

`let` at the top level is a module value. `let` inside a body is a local. The position decides, not a different keyword.

`pub` is allowed on named functions, types, fields, variants, methods, and associates. It is not allowed on module values.

## Functions

There is one function form, with four ways to bind it:

- **Named function**: owns a name in the value pool. It captures nothing.
- **Anonymous function**: a closure literal. It owns no name. It is contained by the function or module value whose body writes it. It owns its parameters and locals, and captures the enclosing locals it reads.
- **Method**: a function in `impl T` whose first parameter has type `T`. It is called as `value.name(arguments)`. The first parameter is the receiver. The name `self` is convention; any name works.
- **Associate**: a function in `impl T` whose first parameter is not `T`, including no parameters. The other parameters do not decide. It is reached as `Type.name(arguments)` or used as the value `Type.name`.

A method's first parameter is passed by every call. A body that ignores it does not warn as an unused local.

A closure literal looks like this:

```
let add_one: fn(int) -> int = [value: int] -> int {
  return value + 1;
};
```

The closure captures the enclosing locals it reads. The capture is by value, so assigning to a captured local inside the closure is rejected.

Multiple `impl` blocks for one type merge into one impl scope. A name may repeat on different types. It may not repeat inside one type's impl scope; methods and associates share that scope.

Functions can have type parameters. The compiler fills them in from the arguments or from the expected type. There are no trait bounds.

A function with no return type can use `return;` or just fall through. A function with a return type must return a value on every path, or end in `panic;` or a call that stops the run.

## Values and mutability

A local `let` is mutable. "Mutable" means you can assign it a new value later, not that you can declare it twice:

```
let count: int = 0;
count = count + 1;
```

This is reassignment. It is not a new declaration. Redeclaring the same name in the same scope is an error. A parameter is a local, so it can be reassigned too.

A local declared in a deeper scope belongs to that scope. It does not exist outside it, so it cannot be reassigned outside it.

A top-level `let` is a module value, not a local. It is evaluated once at startup, in dependency order. A cycle between module values is an error. A module value is never `pub` and it is not assignable.

There is no `mut` keyword and no immutable local option.

## Types

The type groups are:

- `txt`, `bool`, `int`: the scalars.
- `rec`: a bundle of named fields.
- `enum`: one of several named variants, each with its own payload.
- `fn(parameters) -> ReturnType`: a function type.
- `fn(parameters)`: a function type with no return value.
- A type parameter, such as `T` inside `rec Box<T>` or `fn identity<T>(value: T) -> T`.

A declared type is one declaration. A reference to it is an edge to that declaration, not a second type identity.

`Vec<T>` is an ordinary declared `rec` in `std::dynamic`. It is not an intrinsic, it has no literal, and it has no index syntax. You build and change it with its operations:

```
use std::dynamic::Vec;

let values: Vec<int> = Vec.new().push(1).push(2);
let size: int = values.len();
let first: int = values.at(0);
```

`Option<T>` is an ordinary declared `enum` in `std`:

```
std::Option.some(1)
std::Option.none
```

There are no floats and no characters. `txt` is the text type.

There are no implicit conversions. A value fits a type or it does not.

## Expressions and statements

Expressions include:

- text, including interpolation: `"hello @(name)"`
- `true` and `false`
- integer literals
- record literals: `{ name = "api", port = 8080 }`
- enum variants: `Option.some(1)`
- names and paths: `value`, `root::kiru::usage`
- calls: `name(arguments)`, `value.method(arguments)`, `Type.associate(arguments)`
- field access: `value.field`
- binary operators: `+`, `-`, `*`, `/`, `%`, `<`, `<=`, `>`, `>=`, `==`, `!=`, `&&`, `||`
- unary operators: `-`, `!`
- match expressions
- closure literals: `[parameters] -> ReturnType { ... }`

Statements include:

- `let name: Type = value;`
- `name = value;`
- `name.field = value;`
- an expression used as a statement, usually a call
- `return value;` and `return;`
- `panic;`
- `async call;`
- `wait;`
- `for { ... };`
- `break;`
- a match statement

A call always has parentheses. A bare name is always a variable.

`for` is the infinite loop form. There is no `while`, no `for value in ...`, and no `if`/`else`. Branching on a `bool` is done with `match`:

```
match ready {
  true => { std::print("ready"); };
  false => { std::print("waiting"); };
};
```

## Match

A match has a subject and one or more arms. Patterns can be:

- a value, such as `0` or `"ok"`
- an enum variant, such as `Option.some(value)` or `Option.none`
- the wildcard `_`

A match expression must cover every value its subject can take. A match statement records whether it is exhaustive; the flow rules use that to decide whether the statement can fall through.

`_` is only the wildcard in a pattern. Outside a pattern, the normal name and type rules apply.

## Warnings and errors

An error withholds the output. It does not stop the analysis early: the compiler reports every error it can still find, then writes no binary.

A warning never withholds the output. Every warning is reported, and warnings are not configurable.

The warnings are:

- an unused `use`
- an unused local binding
- a program declaration that no reachable body reads

A method's first parameter is its receiver. It is passed by every call, so a body that ignores it does not warn.

## Natives and the runtime

A native is the runtime's floor. It is public, it has no body, and it has no source file. It is called like a function.

- A native that touches the operating system lives in the `syscalls` namespace and carries its syscall or C library name.
- A pure native stays with its kind: containers in `std::dynamic`, text kernels in `std::text`.
- A native is bound to a type when its first parameter is that type, or when it takes no parameters and returns a declared type.
- Every directory under the library root has a `natives.md` listing that namespace's natives.

`Vec`, `Map`, `Set`, and `Option` are ordinary declarations, not natives and not intrinsics. Their methods may call natives underneath.

The runtime executes bytecode. It owns the virtual machine, native dispatch, values, and the process, thread, and signal pools.

## `main`

`main` is not a special declaration form. It is an ordinary named function in the root namespace, and the compiler selects it as the starting point.

The only entry-specific facts are the process interface:

- `main` takes at most one `Vec<txt>` parameter, which carries the command-line words.
- `main` returns `int` or nothing.
- The root namespace's `main` is the entry. A `main` in a sub-namespace is an ordinary function.

The rest of `main` is a normal function: the same parameters, the same body checking, the same return rules, and the same lowering. There is no special declaration form and no separate checking path for `main`; the compiler selects it as the entry and applies the entry contract there.

## Rust comparison

This table is only for the places where Kiru differs from Rust. If a concept is not listed, assume it works the same way or that Kiru does not have it.

| Rust                             | Kiru                             | What is different                                                                     |
| -------------------------------- | -------------------------------- | ------------------------------------------------------------------------------------- |
| `struct`                         | `rec`                            | A `rec` is a named bundle of fields.                                                  |
| `enum`                           | `enum`                           | Same idea: one of several variants, each with its own payload.                        |
| `trait`                          | none                             | There are no traits, interfaces, or trait objects.                                    |
| `impl Trait for Type`            | none                             | `impl Type` only adds inherent methods and associates.                                |
| `mod`, `crate`                   | directory namespace, `kiru.toml` | Directories are namespaces; the manifest names the project tree.                      |
| `use`                            | path shortening                  | A name brings its type and function together; there is no `as`.                       |
| `pub(crate)`, `pub(super)`       | `pub` or private                 | Visibility is one bit.                                                                |
| `String`, `&str`                 | `txt`                            | One text type.                                                                        |
| `i8` ... `i128`, `u8` ... `u128` | `int`                            | One integer type.                                                                     |
| floats, chars                    | none                             | Kiru has `txt`, `bool`, and `int`, and nothing else.                                  |
| arrays, slices                   | `Vec<T>`                         | No fixed arrays, no list literal, no indexing, no slicing.                            |
| `Option<T>`                      | `std::Option<T>`                 | Constructed as `Option.some(value)` or `Option.none`.                                 |
| `Result<T, E>`, `?`              | none                             | Stop with `panic;` or use `Option` and match it.                                      |
| `Box`, `Rc`, `Arc`               | none                             | No source-level pointer or reference types.                                           |
| `&T`, `&mut T`, lifetimes        | none                             | No borrowing, no ownership, no lifetimes.                                             |
| `Type::new()`                    | `Type.new()`                     | `.` is for association; `::` is for namespace paths.                                  |
| `fn method(&self)`               | `fn method(self: Type)`          | The first parameter's type decides method vs associate; `self` is only a name.        |
| `if`, `while`, `for x in`        | `match`, `for { ... }`           | There is no `if`, `while`, or `for-in`.                                               |
| closure `                        | x                                | x + 1`                                                                                | `[x] -> int { ... }` | Brackets are the closure delimiters. |
| `async` / `.await`               | `async call;` / `wait;`          | Async is a statement form, not a value type.                                          |
| `panic!`                         | `panic;`                         | A statement, not a macro.                                                             |
| macros, attributes, derives      | none                             | No macro system and no attributes.                                                    |
| `const`, `static`                | top-level `let`                  | A module value is evaluated once and is not `pub`.                                    |
| `mut` local                      | `let` local                      | Locals are mutable by default; redeclaration is rejected.                             |
| `main`                           | ordinary named function          | The compiler selects it as the entry; the process interface is the only special part. |

## Things Kiru does not have

This list is here so nobody accidentally assumes Rust behavior:

- no traits, inheritance, or dynamic dispatch
- no fixed arrays, slices, list literals, or indexing
- no pointers, references, borrowing, ownership, or lifetimes
- no floats or characters
- no `null`
- no `mod`, `import`, or `as`
- no `mut`
- no `if`, `while`, or `for-in`
- no macros, attributes, or derives
- no operator overloading
- no exceptions; `panic;` stops the run
- no `pub` module values
- no cross-namespace `impl` blocks
