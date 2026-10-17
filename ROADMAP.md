# Kiru Roadmap — toward typed orchestration

A living design log. Decisions are added as they are made; open questions are
tracked so they are settled before they block a phase.

## North star

- The **intrinsic surface** should be the syscall floor plus the smallest kernel
  needed to define everything else in kiru. Everything above the floor is kiru.
- **Kiru is the spine, not the muscle.** Its job is to wire what already exists
  into one typed, deterministic pipeline, not to reimplement it. The muscle is a
  Rust binary, a Go service, a Python script, a shell tool, or an OS primitive;
  kiru runs it, reads what it produces, and decides what comes next.

  ```text
  Rust binaries ─┐
  shell tools   ─┤
  Go services   ─┼──►  Kiru  ──►  wired pipeline
  Python scripts─┤       │
  OS primitives ─┘       └── typed, deterministic, <1 MiB
  ```

  Kiru therefore does not have to be self-sufficient. The question for every
  addition is **whether it helps kiru do its job** — wiring what exists into one
  typed, deterministic pipeline. Size is not the test and composability is not
  the test: a helper earns its place however many lines it takes, if a kiru
  program doing kiru's work needs it, and one that only makes kiru a better
  general-purpose language does not, however few lines it is. Usage is not the
  test either — that something *would* be used says nothing about whether it
  serves the goal.
- Two filters for every feature:
  1. Does it let us **delete a native** (or hold the surface flat)?
  2. Does it make orchestration programs **smaller and clearer**?
- Purpose is unchanged: **task running and data orchestration with
  performance**. The type system is infrastructure, not the product.
- **Not a general-purpose language.** A feature earns its place by serving
  orchestration programs, not by existing in Rust. Rust parity is a coherence
  tool, not a goal: where a Rust feature only serves general-purpose
  programming — recursion for arbitrary algorithms, elaborate inference — it is
  out of scope until an orchestration program actually needs it.

## Design principles

- **Borrow concepts, not implementations.** Anchor the everyday surface on one
  language for coherence (Rust for the type/control/pattern surface), and borrow
  narrowly elsewhere for specific concepts.
- **An unstated rule is a question, not a guess.** When the design does not say
  how something behaves, either ask, or answer it from Rust — Rust is the model
  the surface borrows from — and then tailor the answer to orchestration. A rule
  that *is* stated is not a question: it is enforced, on every path, or it is a
  bug.
- **Rust-like by default, complexity-guarded.** Mirror Rust's surface — builtin
  and named types, the builder pattern, syntax in general — as the default. The
  limit: the moment matching Rust would **require us to deliberately add
  complexity just to match**, re-evaluate. Coherence is the goal, not fidelity.
  (Example: full bidirectional inference is Rust-like but complex; keep the
  simpler argument-driven inference unless a real program needs more.)
- **Take Rust's type surface; skip its memory model.** No ownership, lifetimes,
  or borrow-checking — use a GC / refcount / copy model. Orchestration programs
  are short-lived.
- **Text is the OS boundary type.** Interpolation `@()` is the one formatting
  context; everywhere else types are exact (no implicit coercion).
- **Drop `+` text concatenation.** `@()` already builds text, so `+` is
  arithmetic-only and needs no overloading. Implemented in Phase 1.
- **Interpolation is the one formatting context.** `@(…)` inserts text, an
  integer (decimal), or a boolean (`true`/`false`); everywhere else types are
  exact. Interpolation is not a coercion: it builds `txt` — the literal's own
  type — from its parts, so no value is converted into a type it is not, and the
  result is always `txt`. Implemented in Phase 1.
- **No `if`.** `match` stays the decision construct, branching on a `bool` with
  `true`/`false` arms. A boolean match that covers both needs no `_` arm.
- **Operator spelling:** `&&`, `||`, `!` for logic; `+ - * / %` arithmetic;
  `< <= > >=` ordering; `== !=` equality. All left-associative; `!`/`-` prefix.
- **Reserved words are the forms, not the type names.** The reserved words are
  `mod`, `import`, `use`, `pub`, `fn`, `let`, `rec`, `enum`, `match`,
  `return`, `panic`, `async`, `wait`, `for`, `in`, `break`, `true`, and `false`.
  There is no `type` keyword: the runtime type names (`txt`, `Vec`, `bool`,
  `int`) are built in, not declared — see the intrinsic-type decision below.

## Decisions locked

- **Bindings are assignable by default; there is no `mut`.** A `let` local and
  every parameter may be reassigned, take field assignment (`r.a = v`), and be
  written in place as a vec (`xs[i] = v`). A module-level value is a constant
  and cannot be assigned. (Dropped `mut`: immutability-by-default is Rust's
  borrow-checker-era baggage; orchestration wants the simple rule.)
- **`rec` keyword** for a nominal type declaration.
- **No `=` in a type declaration** — it defines a shape, not an assignment:
  `rec Name { field: Type, ... };`
- **A declaration defines a type and its shape; every value is a `let`.** `rec`
  and `enum` declare types, the built-ins (`txt`, `int`, `bool`, `vec<T>`) are
  types, and every value is `let name: Type = shape;` where the shape is the
  literal form — `""`, `0`, `true`, `[]`, `{}`, a variant. No literal carries
  its own type name, so `Name { … }` is gone. (Uniform across every type, and
  it drops a node from the backend.)
- **The CLI is a command tree written in kiru, and everything is a method.**
  `std::cli` declares commands and flags by chaining — `command("build")
  .about("build it").flag(switch("--verbose").short("-v").help("more"))
  .action(run)` — links children with `add`, and `tree().parse(words)` descends
  by the leading words, accumulates the ancestors' flags into a `Context`, and
  runs the leaf's action. `usage` is a method too. A command's description is
  `about` and a flag's is `help`: different concepts, different words. A method
  name may now repeat once per receiver head, so `Command.about` and
  `Flag.about` could coexist, but a description and a help line are different
  things and keep different names. The readers take a `Flag`
  *value*, not a string, so a flag is declared once. `main` takes the words, so
  argv is no longer a flat record.
- **A process has one way to run, and I/O has one method.** `std::process` has
  exactly one way to run a program: `command(line)` returns a `Command` you
  chain and finish with `.run()` (the exit code as an `int`) or `.capture()`
  (the captured streams as data). There is no `run`/`capture` free function
  taking a spec record — that was a second and third way. I/O is one method:
  `.stream(stream, target)` changes where one stream goes, and *not* calling it
  streams normally (the child inherits), so there is no stringly-typed mode
  table and no separate `stdin`/`stdout`/`stderr`/`quiet` methods. Every method
  says one thing and takes the type it means — a timeout, an exit code, and a
  pid are `int`s, not text. A `rec` plus chained methods is how anything is
  configured; a `Map<K, V>` (in `std::map`) is how data is passed and returned.
- **`( ... )` groups an expression.** `(a + b) * c`. The parentheses carry no
  node: they only steer precedence, which the tree already records. (Added:
  without them the expression grammar had no way to group at all.)
- **The type system closes at the intrinsics and the two formers.** The
  intrinsic types are `int`, `bool`, `txt`, and `vec<T>`; the two type formers
  are `rec` and `enum`. `map<K, V>` is **not** a built-in: it is an ordinary
  declared type in the `std::map` category (`stdlib/map.kiru`), a `rec` over a
  `vec` of `Entry<K, V>`, so `use std::map;` brings the type and its methods
  (`get`/`set`/`has`/`keys`/`values`/`len`, plus `from`/`empty`) exactly as
  `use std::vec;` brings `vec`. Nothing about it needs a native — the runtime
  represents a `Map` like any other rec. A field read `r.field` is only for a
  field of a declared `rec`; the second rule — `m.k` on a map, yielding
  `Option<V>` — is gone, and a map lookup is `m.get(k)`. A `{ k = v }` literal
  is only ever a declared-type literal, so it still needs an expected type and a
  `{ … }` with none is an error. (Supersedes the earlier decision that
  `map<K, V>` replaced the anonymous record: there is no anonymous record type
  at all, and `map` is a stdlib rec rather than a runtime intrinsic.)
- **A type name is an ordinary name; the intrinsics are built in, not
  declared.** Every written type — `txt`, `Vec<T>`, `bool`, `int`, a declared
  `rec`/`enum`, a type parameter — parses as one `TypeExpr::Named` and resolves
  through `Builder::resolve_named_type`. `resolve_named_type` checks
  `BuiltinType::from_name` for a bare, non-rooted, single-segment name before it
  looks up any declaration, checks the type-argument count with
  `parameter_count()`, resolves the arguments, and returns
  `builtin.runtime_type(&resolved)` directly: no declaration, no `TypeId`,
  nothing to delete. The runtime's types cannot go missing because they are the
  language's own. There is no `type` keyword and no bodyless declaration form;
  both `type Foo;` and `type txt;` are parse errors. (Removed the
  `txt`/`bool`/`int` keyword table, the `vec` token, the contextual `map`
  lookahead, the `type` keyword, and the
  `TypeDeclarationKind::Builtin`/`TypeKind::Builtin`/`builtin_kind`/
  `is_std_category` machinery: one grammar and one resolution path, and the
  intrinsic surface is not a declaration.)
- **The intrinsics are visible in every namespace; everything else is
  imported.** `txt`, `bool`, `int` and `Vec<T>` are the language's own types, so
  a program writes them without a `use`. Every other name is reached exactly as
  any other name is: a bare name sees its own namespace plus what it `use`s, and
  a method needs its category in scope (`xs.len()` needs `use std::vec;`),
  because dispatch is a separate rule from naming a type. `fn(T)` with no arrow
  is a structural function type (`TypeExpr::Function { returns: None }`), so
  writing a function type needs no `use`. (Reversed the earlier "no prelude"
  decision, which made a built-in reachable only through a `use`.)
- **A bare name sees its own namespace plus the namespaces it `use`s; nothing
  climbs to a parent.** A declaration reached across namespaces must be `pub`; a
  qualified path or a rooted `::` path names a namespace directly and reaches
  only `pub` declarations. The rule is the same for a `rec` field and an `enum`
  variant: **every declaration, field, and variant is private by default** —
  reachable in its declaring namespace and across namespaces only when marked
  `pub`. `pub` precedes a field (`pub field: T`) and a variant (`pub Variant(T)`)
  exactly as it precedes a declaration. (Closed a gap: the check covered
  declarations only, so `m.entries[0].key` read and `hidden::Colour.red`
  constructed across namespaces without a `pub`.)
- **`main` takes the words and returns the exit code.** `fn main(words: vec<txt>)
  -> int` — the returned `int` is the process status, clamped to `0..=255`; no
  return means `0`; any other parameter or return type is an error. (Replaced a
  numeric-text status: the exit code is an `int`, not a number smuggled through
  text, and a failure is `std::eprint`.)
- **Text is UTF-8, not raw bytes.** Orchestration reads text — configs, logs,
  manifests, source — so the boundary type is a UTF-8 string and `read` refuses
  a file whose bytes are not UTF-8; a binary artifact is a child process's job,
  not the language's. `slice` therefore refuses a range that splits a character.
  (Considered `Arc<[u8]>`: it would delete the char-boundary rule and let kiru
  read a tarball, but no orchestration program needs that, and byte text would
  push UTF-8 validation into every consumer.)
- **`import` and `mod` are the coherence mechanism.** A file's `import`s say
  which files are in the program and in what order, and an import cycle is an
  error; a `mod` block says which namespace a file's declarations join. Together
  they fix the declaration order — a file's own declarations, then the files it
  imports, in the order written — and a reference may only point backwards in
  that order. A circular *reference* is therefore a circular *import*, which is
  already rejected; that is how circular dependencies are disallowed, not by a
  rule of their own, and no separate call-graph check is needed. A file may also
  extend a namespace another file declared — `mod std` beside `mod std::process`
  — which is how a later file adds to `std` without the earlier one importing it
  back. The inline `mod` block is a feature of the mechanism, not a wart.
- **A `use` may only reach an earlier file.** A `use` obeys the same backwards
  rule the resolver applies to a name, extended across files: it may only reach
  a namespace whose declarations come before it in the joined stream — that is,
  a namespace in a file loaded earlier than the `use`, or in the `use`'s own
  file before the `use`. A file cannot reach a namespace from a file it never
  imported, or imports later: `std.kiru` may not `use std::vec;` while
  `vec.kiru` is imported below it. The check lives in
  `Builder::reserve_items`, comparing the `use`'s position in the joined stream
  to the position of the namespace's first declaration, and it names the
  declaring and writing files when it rejects one. `ScopedUse` carries the
  writing file for that diagnostic.
- **A type may name itself, and any other type.** Every declaration is reserved
  before any field is resolved, so a type may be self-referential or mutually
  recursive: a command tree (`children: vec<Command>`), a document
  (`enum Json { Array(vec<Json>), Object(vec<Field>) }` beside `rec Field`), a
  directory tree, a process tree. The checker needs no change — `fits` compares a
  named type by its id and its arguments, never by walking fields, so a recursive
  type cannot make it loop.
- **No slicing.** `xs[i]` reads one element; there is no `xs[a..b]`. Slicing was
  used nowhere in the stdlib, the tests, or the dogfood scripts, so it did not
  serve the domain. (Removed; `std::text::slice` had already gone.)
- **A call is a path call or a method call; there is no `f()()`.** `f(x)` calls
  the function named by a path, and `receiver.m(x)` calls a method. A function
  value is called through a name (`apply(x)`), never by calling a call result
  (`make()()`) or an index (`fs[0](x)`). (Dropped `Expression::Invoke`: an
  arbitrary callee adds no orchestration and reads badly.)
- **The sequence type is `vec<T>`, and its element type is always written.** No
  bare `vec`, and no implicit `vec<txt>`. The type and its helpers share the
  `std::vec` namespace (`stdlib/vec.kiru`), so `use std::vec;` brings in both.
  (Renamed from `list`; the namespace was `std::vecs` while `vec` was a
  keyword.)
- **Generic type declarations:** `rec Name<A, B> { first: A, second: B };`
- **Generics stay; a container carries its element type.** `vec<T>` and
  `Option<T>` keep their parameter, and the element type flows statically through
  `xs[i]`, `for x in xs`, `map`/`filter`/`sort_by`, and an `Option<T>` payload.
  The type system does not gain a dynamic element type. (Considered replacing the
  element type with one dynamic type; rejected — the syntax already answers "what
  is an element" by writing the type, and kiru is statically typed.)
- **Surface: `:` for types, `<>` for type parameters, `=` to assign a value.**
  `let p: Project = { name = "x" };`, `fn f(a: txt) -> txt`, `rec P { a: txt }`,
  and `vec<txt>` for generics. Record literal fields keep `=`. A literal never
  names its type: a declaration defines a type and its shape, and every value is
  `let name: Type = shape;` (`let p: Project = { … };`).
- **`.` reads a member of a value or a type; `::` reads a module.** Field access
  (`r.a`), a method call (`xs.len()`), and variant construction and patterns
  (`Option.some`, `Shape.Circle(2)`) all use `.`; a module path
  (`std::vec::sort`) uses `::`. A variant is told from a field by the root: a
  type name is a variant, a value is a field. (Dropped `Name::Variant`: one
  separator for "a member of something", so `Option.some` mirrors `r.field`.)
- **Duplicate record fields always error**, Rust-like: a repeated field name is
  an error in any record literal, mutable or not. (A spec built field by field
  will use a builder pattern instead.)
- **Direction: mirror Rust's type system and syntax** — with `int` in place of
  sized integers — while the standard library stays high-level and
  data-orchestration oriented.
- **Logical operators are symbols:** `&&`, `||`, `!` (identifiers cannot hold
  special characters, so symbols stand out).
- **No `Result`.** A fallible call is an ordinary value; the bad case is handled
  by a wildcard match arm that errors (`std::eprint`/`panic`). There is no `?`
  and no error-propagation type.
- **`Option` is not built in.** The language gains `enum` (with payloads) and
  variant patterns; `Option<T>` (`some`/`none`) is then defined as an ordinary
  stdlib enum in Phase 3. No built-in optional type.
- **Parser: precedence-climbing table** (Pratt-lite), not necessarily a full
  Pratt parser.
- **Methods: a function whose first parameter accepts the receiver.** The
  parameter's type is the dispatch target and its name is irrelevant. A method
  is an ordinary declaration in a namespace; a call `receiver.name(args)`
  resolves `name` among the namespaces in scope, selects the function whose
  first parameter has the receiver's type head, and desugars to a call with the
  receiver as the first argument. A name may repeat once per receiver head in a
  namespace: a method family shares one name, and the receiver's type head
  selects within the namespace as well as among them. Every other declaration
  still makes a name unique, so a genuine duplicate is an error. First user: the
  `std::command` builder (`std::command(line).dir(...).run()`). Next: the
  vec/text helpers. (Dropped the `impl` block: it was only the grouping device
  that supplied the target type, which the first parameter supplies directly.)

## Open questions

- **Exact operator spelling** for logic (`&&`/`||`/`!` vs `&`/`|`/`!`).
- **Fixed-size arrays:** out of scope for now, or a second sequence type?

## Phases (each reversible)

| Phase | Adds | Deletes / unlocks |
| --- | --- | --- |
| 0 ✅ | Precedence-climbing core (only `+` today) | — (pure refactor; `parse_sum` → `parse_binary`) |
| 1 ✅ | `bool` + `int`, literals, operators (`+ - * / % < <= > >= == != && || !`), typed predicates, `@()` formatting | `is_terminal`/`exists` → `bool`; `text`/`vec`/`path` predicates → `bool`; `+` concat dropped |
| 2 ✅ | nominal `rec` types with typed fields (2a); parameterized `vec<T>` (2b); indexing (2c) | `std::get`/`std::text::slice` (removed); `len`/`find` → `int`; duplicate-field and `_`-ordering warts |
| 3 ✅ | generics (type params) + enums with payloads + variant/binding patterns + exhaustiveness | `Option` defined as a stdlib enum; no `Result` built in |
| 4 | closures/HOF (4b ✅ function values, 4c ✅ closures), mutable indexed sequences (4a ✅) | `sort`/`map`/`filter` in kiru (the O(n log n) case) |
| 5 ✅ | shrink natives to the floor; document it (text kernel stays) | the budget below |

### Phase 2 breakdown

- **2a ✅ — nominal `rec` types.** `rec Name { field: Type, ... };` declares a
  type; annotations may name it (`: Name`); a record literal is checked against
  the expected type (anonymous `{ … }` stays the flat text record). Adds
  `Type::Named(TypeId)` on the flat, `Copy` `Type`; `TypeId` indexes a program
  type table. Field access has the field's type; `let mut` + field assignment
  respect it. Duplicate fields always error (nominal or anonymous). Record
  values now hold `Value`s so a field can be any type.
- **2b ✅ — parameterized types.** `Type` carries parameters: `Type::List(Box<Type>)`,
  so it is now `Clone` but no longer `Copy`. Native rows keep a small `Copy`
  `NativeType` (`Text`/`Record`/`List`/`Bool`/`Void`) mapped to a `Type` at the
  boundary, so the rows stay `const`. `vec<T>` is written in any annotation; the
  element type is always written. A vec literal is checked element-by-element
  against the expected `vec<T>` (contextual, like records), and `for item in xs`
  binds the item to the element type. Runtime vecs hold `Value`s
  (`Value::Vec(Vec<Value>)`). Most standard-library vec helpers (`std::vec::append`,
  `prepend`/`concat`/`reverse`/`contains`, `std::join`, `std::last`) are still
  monomorphic on `vec<txt>`; they wait on `impl` so they can become methods
  (see the backlog). `len` is generic.
- **2c ✅ — indexing, `len` → `int`.** `xs[i]` reads a vec element at an `int`
  index; an out-of-bounds index reports a message. `std::text::len`,
  `std::vec::len`, and `std::text::find` now return `int`; `std::get` is
  removed (use `xs[i]`). Still to move:
  `std::vec::append`/`std::vec::prepend` wait on Phase 4's mutable indexed
  sequences (there is no way to build a vec in kiru yet).

### Phase 3 breakdown

- **3a ✅ — enums.** `enum Name { A, B(Type), };` declares a nominal enum.
  `Name.A` and `Name.B(x)` construct a variant (`Expression::Variant`,
  lowered to `MakeVariant`; `Value::Variant { tag, payload }`). A `match` arm
  may name a variant and bind each payload position to a fresh local or `_`
  (`Pattern::Variant`; `JumpIfVariantNotEqual` + `GetVariantField`). A match on
  an enum must cover every variant or end with `_`; a repeat variant is a
  duplicate; a variant pattern on the wrong subject is an error. `Pattern`
  became its own node (`Value` | `Variant`), so match arms no longer reuse
  `Expression`; visitors gained a `pattern` hook. Non-generic enums only.
- **3b ✅ — generics.** Types are parameterized (`rec Name<A, B>`, `enum
  Option<T>`, application `Name<int>`) with substitution across fields, variant
  payloads, construction, matching, and field access. Functions are generic too
  (`fn length<T>(xs: vec<T>) -> int`): the call infers the type arguments by
  unifying each parameter type against its argument's type. `Type` gained
  `Named(TypeId, Vec<Type>)` and `Parameter(String)`; resolution takes a
  parameter scope; inference uses `unify`.
- **3c ✅ — stdlib `Option<T>`.** `enum Option<T> { some(T), none }` is defined
  in `stdlib/option.kiru`, with `std::unwrap_or`/`std::is_some`. `std::text::find`
  now returns `Option<int>` (the raw kernel is `std::text::find_byte`; the kiru
  wrapper matches on the `-1` sentinel). `std::vec::len` moved out of the
  native kernel into kiru as a generic `fn len<T>(xs: vec<T>) -> int`. `require`
  now flows an expected type into a match's arms, so a generic construction or a
  record literal can take its type from the match it sits in.

### Phase 4 breakdown

- **4a ✅ — mutable indexed sequences.** A vec takes `xs[i] = value`:
  in range it replaces, and at `i == len(xs)` it appends (the `SetIndex`
  instruction mutates the register's vec in place). With in-place building,
  `std::vec::append` and `std::vec::prepend` left the native kernel for kiru.
- **4b ✅ — function values + HOF.** Functions are values: a `fn(T) -> R` type,
  function-value expressions, calls through a function value (`CallValue`), and generic call-site
  inference through a function argument. `map`/`filter` moved into
  `stdlib/vec.kiru`. With `<` extended to `txt`, `sort` moved too: it is an
  iterative O(n log n) bottom-up merge sort, and `sort_by` takes a comparator.
- **4c ✅ — closures.** An anonymous `fn(a: T) -> R { ... }` literal is a value.
  It captures the enclosing locals **by value** at creation (capture-all, so no
  free-variable analysis): the closure's leading registers are the captured
  values, `MakeFunction` snapshots them, and a captured local is read-only
  inside the closure. Nested closures capturing across a boundary work, a
  closure body is checked inline so a captured loop variable keeps its type, and
  `async` accepts a closure call. A closure with no return type is `fn(params)`
  and returns no value.

### Backlog — everything planned but not yet built

A durable backlog, so no plan is forgotten. Each item is filtered by the north
star: does it delete a native (or hold the surface flat), and does it make
orchestration programs smaller and clearer?

1. **3c ✅ — stdlib `Option<T>`.** `enum Option<T> { some(T), none }`; retype
   fallible helpers onto it (`std::text::find` returns `Option<int>`). A generic
   `len` in kiru replaced the monomorphic `std::vec::len` native.
2. **Methods ✅, and the stdlib is migrated.** A method is a function whose first
   parameter accepts the receiver (its name is irrelevant; top-level or inside a
   `mod`); `receiver.name(args)` dispatches at link time on the receiver's
   declared type head. The stdlib now
   uses methods: `std::command` is a builder; `vec<T>` has `len`/`append`/`prepend`/
   `concat`/`reverse`/`contains`/`last`/`map`/`filter`/`sort_by`; `txt` has
   `len`/`byte`/`trim`/`find`/`contains`/`starts_with`/`ends_with`/`replace`/`lines`;
   `Option<T>` has `unwrap_or`/`is_some`. `std::join` (a text convenience) and
   `std::vec::sort` stay free functions, since each applies to `vec<txt>` alone
   rather than to every `vec<T>`.
3. **Phase 4.** 4a ✅ mutable indexed sequences (`std::vec::append`/`std::vec::prepend`
   moved into kiru). 4b ✅ function values + calls through a function value (`std::vec::map`/
   `filter`/`sort`/`sort_by` moved into kiru; `<` extended to `txt`). 4c ✅
   closures, capturing only the free variables the body uses (nested closures
   propagate the ones they need).
4. **Phase 5 ✅ — the floor is documented, and revised.** The kernel is now the
   byte access `len`/`byte`/`find_byte`/`slice`. `trim` and `split` left the
   kernel for kiru (they were Unicode policy — ASCII now, and a degenerate empty
   needle is an error), and `glob` left for kiru over the new `list` (it was a
   Rust crate, not a syscall). Net: 15 syscalls + a 4-native byte kernel. See
   the budget table below.
5. **Reconcile-later warts (deferred — each needs a syntax/design call).**
   (a) ✅ The literal never names its type: `Expression::Struct` (`Name { … }`)
   is gone, so a `{ … }` shape is typed by its annotation and the brace
   ambiguity is gone with it. (b) ✅ `rec` only declares; there is no anonymous
   record type, and the key-valued container is the ordinary `std::map::Map`.
   (c) ✅ A generic
   literal cannot carry type arguments, which no longer arises: the literal
   carries no type at all.
6. **Small items.** A `let` element-type shorthand is deferred sugar
   (`let xs: vec<int> = …` is the form today; the shorthand is unspecified).
7. **Open questions — settled.** Logic operators stay the symbols `&&`/`||`/`!`.
   Fixed-size arrays are out of scope; `vec` covers sequences. Recorded so the
   questions do not reopen.
8. **Argument-aware method dispatch (future).** Dispatch keys on the type head
   only, so there is one `impl` per type and a text-specific `sort`/`join` cannot
   be a `vec<T>` method. Matching type arguments (most-specific impl) would let
   them become methods.

### Deferred cleanups (from the backend audit)

- ✅ The stdlib `Nix` flag is now `bool`, through a declared `rec Project` in
  `main.kiru`; `wrap` matches `project.Nix` on `true`.
- ✅ Stale comments from earlier phases (`switch`, "defer", "internal number",
  "number kind", and the type-system "kind") are swept.

### Reconcile later (Rust warts to improve on)

- ✅ A record literal works as a `match` subject or a `for` iterable (Rust needs
  parens), and no literal names its type.
- ✅ `rec` declares a nominal shape; the key-valued container is the ordinary
  `std::map::Map`, a `rec` over a `vec` of entries.

## Intrinsic-surface budget

The native surface is the rows in `src/native_registry.rs`, tagged by
disposition. The floor is the OS boundary; the rest moves into kiru as the core
gains capability.

| Native | Role | Disposition |
| --- | --- | --- |
| `std::process::spawn` | `fork`/`exec` | floor |
| `std::process::reap` | `waitpid` | floor |
| `std::process::signal_group` | `kill` | floor |
| `std::io::write` | `write` | floor |
| `std::io::is_terminal` | `isatty` | floor |
| `std::io::read_stdin` | `read` | floor |
| `std::fs::read` | `open`/`read` | floor |
| `std::fs::write` | `open`/`write` | floor |
| `std::fs::remove` | `unlink` | floor |
| `std::fs::exists` | `stat` | floor |
| `std::fs::mkstemp` | `mkstemp` | floor |
| `std::fs::list` | `getdents64` | floor (**added**) |
| `std::time::sleep_seconds` | `nanosleep` | floor |
| `std::env::var` | `getenv` | floor |
| `std::env::current_dir` | `getcwd` | floor |
| `std::text::len` | byte length | kernel |
| `std::text::byte` | byte read | kernel |
| `std::text::find_byte` | byte search | kernel |
| `std::text::slice` | byte range | kernel (**added**) |
| ~~`std::fs::glob`~~ | a Rust crate | **deleted** — kiru over `list` |
| ~~`std::text::trim`~~ | Unicode policy | **deleted** — kiru over `byte`/`slice` |
| ~~`std::split`~~ | Unicode policy | **deleted** — kiru over `find_byte`/`slice` |

Floor: 15 syscalls + a 4-native byte kernel. The rule: **a native earns its
place by being inexpressible in kiru, not by being convenient.** A high-level
convenience in the kernel — a `whole` native, say — is the drift to refuse: it
would make the next one look reasonable. The kernel is surfaced as methods
(`s.len()`, `s.byte(i)`, `s.slice(a, b)`, `s.find(needle)`), so the everyday
surface is uniform. (Moved into kiru earlier: `std::vec::len` in 3c;
`std::vec::append`/`std::vec::prepend` in 4a; `std::vec::sort` in 4b, with `<`
extended to `txt`; then the whole `std::vec`/`std::text`/`Option` helper set
became methods in the `impl` phase.)

### The minimization: which native deletes the most

Two additions delete three natives, and every deletion is one that was never a
syscall — which is the test the floor should have been applying all along.

1. **`std::text::slice(text, start, end) -> txt`** — the bytes in `[start, end)`.
   It is the one text operation kiru cannot express: building a substring with
   `@()` costs a rope node per piece, while a slice copies exactly its range.
   `find_byte` stays, because a kiru byte loop would be O(n) VM dispatches where
   a native search is O(n) machine bytes — that one native is what keeps `split`
   linear. With `slice`, `trim` and `split` move to kiru:
   - `split(t, sep)` finds each separator with `find_byte` in the O(1) remainder
     `t.slice(index, len)` and slices the piece between — O(separators) native
     calls, linear overall, the same asymptotic cost as the native.
   - `trim(t)` walks `byte` from each end while it is ASCII whitespace, then
     slices — O(the trimmed run) native calls, usually a handful.
   The behavior change is the point: Unicode whitespace and the char-boundary
   empty-separator split are Rust library *policy*, not the language. Text at the
   OS boundary is bytes, and an empty separator is an error. (Slicing as
   *syntax*, `xs[a..b]`, stays removed; `slice` returns as an ordinary `txt`
   method, because it is the operation that deletes two policy natives.)
2. **`std::fs::list(path) -> vec<txt>`** — the entry names in a directory
   (`getdents64`). It deletes `glob`, which is a Rust crate, not a syscall — the
   clearest case of library logic above the floor. Glob becomes kiru: split the
   pattern on `/`, descend a literal segment directly, `list` and match a
   wildcard segment, `**` descends recursively, and the result is `sort`ed. It
   also makes a recursive walk, a directory tree, and a file picker expressible
   in kiru for the first time.

Candidates rejected, with the reason: a bulk `bytes(text) -> vec<int>` would
delete `byte`/`find_byte`/`trim`/`split` at once but pays an O(n) allocation on
every text operation (a 1 MB `contains` would build a million-element vec), so it
is slower, not equal; a unified `open`/`read`/`write`/`close` handle would
delete `fs::read`/`fs::write`/`read_stdin`/`io::write` but adds a resource
lifetime the language does not have (no ownership); a raw `syscall(nr, args)` is
not a language.

## Parser note

The expression parser is `parse_expression → parse_sum (+) → parse_postfix (.) →
parse_primary`. With a real operator set, replace `parse_sum` with a
precedence-climbing loop over a small `token → (precedence, associativity)`
table, keeping recursive descent for primaries and the postfix loop for `.`,
`()`, and `[]`. `<` is disambiguated by position: type-argument open in type
positions, comparison in expression positions.

- **Vec type syntax: `vec<T>`.** The canonical vec type is `vec<T>`, used in
  every position (`let tags: vec<txt> = [...];`, `fn f(tags: vec<txt>)`). The
  literal shape (`{}`, `[]`, `""`) must match the type — the "hard contract" is
  a check, not the source of the container. An element-type shorthand in `let`
  is deferred sugar, not a rule.
