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
  `use`, `pub`, `fn`, `impl`, `enum`, `let`, `rec`, `match`, `return`, `panic`,
  `async`, `wait`, `for`, `break`, `true`, and `false`. There is no `type`
  keyword: the intrinsic type names (`txt`, `bool`, `int`) are built in, not
  declared — see the intrinsic-type decision below — and `Vec` is an ordinary
  declared `rec` in `std::dynamic`. (`mod`, `import`, and `in` are deleted: a
  directory is a namespace, a file is in the program because it is under the
  entry's directory, and a loop is `for { ... };` alone.)

## Decisions locked

- **A `use` may stand anywhere among a file's top-level items.** Position
  carries no meaning, for declarations and `use`s alike: a `use` shortens paths
  for the whole file's namespace, and the written order between `use`s alone
  decides what a delivered name is on a collision. (Deleted the parser's
  first-declaration gate and its `use must come before any declaration` error.)
- **Bindings are assignable by default; there is no `mut`.** A `let` local and
  every parameter may be reassigned, take field assignment (`r.a = v`), and
  change a vec in place (`xs = xs.replace(i, v)`). A module-level value is a
  constant and cannot be assigned. (Dropped `mut`: immutability-by-default is
  Rust's borrow-checker-era baggage; orchestration wants the simple rule.)
- **`rec` keyword** for a nominal type declaration.
- **No `=` in a type declaration** — it defines a shape, not an assignment:
  `rec Name { field: Type, ... };`
- **A declaration defines a type and its shape; every value is a `let`.** `rec`
  and `enum` declare types, the intrinsics (`txt`, `int`, `bool`) are the
  runtime's own, and every value is `let name: Type = shape;` where the shape is
  the literal form — `""`, `0`, `true`, `{}`, a variant. A vec is built through
  its operations, not a literal. No literal carries its own type name, so
  `Name { … }` is gone. (Uniform across every type, and it drops a node from the
  backend.)
- **The CLI is a command tree written in kiru, and everything is a bound
  function.** `std::cli` declares commands and flags by chaining — `command("build")
  .about("build it").flag(bool_flag("--verbose").short("-v").help("more"))
  .action(run)` — links children with `with`, and `tree.parse(words)` descends
  by the leading words, accumulates the ancestors' flags into a `Context`, and
  runs the leaf's action. `usage` is a method too. A command's description is
  `about` and a flag's is `help`: different concepts, different words. A name
  may repeat only when the functions are bound to different types, so
  `Command.about` and `Flag.about` could coexist, but a description and a help
  line are different things and keep different names. The readers take a `Flag`
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
  configured; a `HashMap<K, V>` (in `std::dynamic`) is how data is passed and returned.
- **`( ... )` groups an expression.** `(a + b) * c`. The parentheses carry no
  node: they only steer precedence, which the tree already records. (Added:
  without them the expression grammar had no way to group at all.)
- **The type system closes at the intrinsics and the two formers.** The
  intrinsic types are `int`, `bool`, and `txt`; the two type formers are `rec`
  and `enum`. `Vec<T>` is **not** an intrinsic: it is an ordinary declared `rec`
  in the `std::dynamic` category (`std/dynamic/vec.kiru`), a `rec` whose
  operations are the `new`/`push`/`replace`/`at`/`len` natives, so
  `use std::dynamic;` brings the type and its methods. `HashMap<K, V>` is a
  declared `rec` beside it (`std/dynamic/hash_map.kiru`) that the runtime keeps
  as a hash table, so its operations are natives and its keys are unordered. It
  is a built-in named type, not an intrinsic. A
  field read `r.field` is only for a field of a declared `rec`; the second rule
  — `m.k` on a map, yielding `Option<V>` — is gone, and a map lookup is
  `m.get(k)`. A `{ k = v }` literal is only ever a declared-type literal, so it
  still needs an expected type and a `{ … }` with none is an error. (Supersedes
  the earlier decision that `map<K, V>` replaced the anonymous record: there is
  no anonymous record type at all, and `HashMap` is a declared rec rather than a
  runtime intrinsic. `Set` is gone: a set is a sorted vec with `dedup`, or a
  `HashMap<K, bool>` for membership.)
- **A type name is an ordinary name; the intrinsics are built in, not
  declared.** Every written type — `txt`, `bool`, `int`, `Vec<T>`, a declared
  `rec`/`enum`, a type parameter — parses as one `TypeExpr::Named` and resolves
  through `Builder::resolve_named_type`. `resolve_named_type` checks the scalar
  name table for a bare, non-rooted, single-segment name before it looks up any
  declaration, checks the type-argument count, resolves the arguments, and
  returns the scalar's runtime type directly: no declaration, no `TypeId`,
  nothing to delete. The runtime's scalar types cannot go missing because they
  are the language's own. `Vec` is not here: it is an ordinary declaration the
  compiler registers in `std::dynamic`. There is no `type` keyword and no
  bodyless declaration form; both `type Foo;` and `type txt;` are parse errors.
  (Removed the `txt`/`bool`/`int` keyword table, the `vec` token, the contextual
  `map` lookahead, the `type` keyword, and the
  `TypeDeclarationKind::Builtin`/`TypeKind::Builtin`/`builtin_kind`/
  `is_std_category` machinery: one grammar and one resolution path, and the
  intrinsic surface is not a declaration.)
- **The intrinsics are visible in every namespace; everything else is reached by
  its path or a `use`.** `txt`, `bool`, and `int` are the language's own types,
  so a program writes them without a `use`. Every other name is reached exactly
  as any other name is: an absolute path resolves on its own, a bare name sees
  its own namespace and the names a `use` brings in, and a `use` only shortens a
  path. `Vec<T>` is a declaration in `std::dynamic`, so a program reaches it by
  its path or with `use std::dynamic::Vec;`. A bound function is gated by `pub`
  alone, so a method call needs no `use`: `xs.len()` resolves by the receiver's
  type head wherever the method is declared. `fn(T)` with no arrow is a
  structural function type (`TypeExpr::Function { returns: None }`), so writing
  a function type needs no `use`.
- **A bare name sees its own namespace only; a qualified path starts at the
  root.** A declaration reached across namespaces must be `pub`; a qualified
  path names a namespace directly and reaches only `pub` declarations, whether
  the path is written in full (`std::dynamic::Vec`) or shortened by a `use`.
  Nothing climbs to a parent. The rule is the same for a `rec` field and an
  `enum` variant: **every declaration, field, and variant is private by
  default** — reachable in its declaring namespace and across namespaces only
  when marked `pub`. `pub` precedes a field (`pub field: T`) and a variant
  (`pub Variant(T)`) exactly as it precedes a declaration.
- **`main` takes the words and returns the exit code.** `fn main(words: Vec<txt>)
  -> int` — the returned `int` is the process status, clamped to `0..=255`; no
  return means `0`; any other parameter or return type is an error. (Replaced a
  numeric-text status: the exit code is an `int`, not a number smuggled through
  text, and a failure is `std::eprint`.)
- **Text is UTF-8, not raw bytes.** Orchestration reads text — configs, logs,
  manifests, source — so the boundary type is a UTF-8 string and `read` refuses
  a file whose bytes are not UTF-8; a binary artifact is a child process's job,
  not the language's. `substr` therefore refuses a range that splits a character.
  (Considered `Arc<[u8]>`: it would delete the char-boundary rule and let kiru
  read a tarball, but no orchestration program needs that, and byte text would
  push UTF-8 validation into every consumer.)
- **A directory is a namespace.** A file belongs to its directory's namespace
  and its filename is arbitrary; nesting nests. The standard library is the
  `std` tree and a program is the `root` tree, so `std/dynamic/vec.kiru` is
  `std::dynamic` and `pipeline/kiru/cli.kiru` is `root::kiru`. A file is in the
  program because it is under the manifest's `project.root`, walked
  recursively; there is no `import`. Files inside a directory may be renamed, split, or
  merged freely, because only the directory names the namespace.
- **A project is a `kiru.toml`.** The manifest names the program tree
  (`project.root`) and the binary's path (`project.output`); the root
  namespace's `main` is the entry. `kc` reads `./kiru.toml` or the manifest it is given, so
  the compilation target is one file rather than a source file. The manifest
  also decides the server's publication scope and which unused warning kinds
  are reported. The standard library is not a project: it is embedded from
  `std/**` and mounted as `std` for every program.
- **A `use` shortens a path; it is namespace-scoped, and resolution is
  order-free.** `use <absolute path>;` is one grammar rule, and what the path
  ends at decides what it shortens: a namespace gives the qualifier its last
  segment spells (`use std::dynamic;` then `dynamic::Vec`); a type gives that
  type bare (`use std::dynamic::Vec;` then `Vec<T>`); a plain function gives
  that one name bare (`use std::fs::glob;` then `glob`). One level of braces
  shortens several names under one prefix (`use std::dynamic::{HashMap, Vec};`).
  There is no alias and no `as`. A `use` sits at the top of its file, ahead of
  any declaration, and covers every declaration of the namespace that wrote it,
  whichever file declared it. It does not reach sub-namespaces and does not
  chain. A `use` never reaches a module value or a bound function: a bound
  function is reached through its type. Resolution is order-free: every
  declaration is collected into its namespace first, so a name may be declared
  after the point that names it and a function may call itself. (Deleted the
  file-position `use` gate, `Builder::check_earlier`, and the structural order
  rule.)
- **`pub let` is an error; module values are private and dependency-ordered.**
  A module value is evaluated once at startup and is never reachable across a
  namespace, so `pub` on a `let` is rejected. Within a namespace, module values
  are ordered by their initializers — a value is evaluated before the values
  that read it — and a cycle among them is an error.
- **A type may name itself, and any other type.** Every declaration is reserved
  before any field is resolved, so a type may be self-referential or mutually
  recursive: a command tree (`children: Vec<Command>`), a document
  (`enum Json { Array(Vec<Json>), Object(Vec<Field>) }` beside `rec Field`), a
  directory tree, a process tree. The checker needs no change — `fits` compares a
  named type by its id and its arguments, never by walking fields, so a recursive
  type cannot make it loop.
- **A vec reads and writes through its operations; there are no index
  brackets.** `xs.at(i)` reads one element, `xs.replace(i, v)` replaces one, and
  `xs.push(v)` appends, so there is no `xs[i]` and no `xs[a..b]`. Slicing was
  used nowhere in the stdlib, the tests, or the dogfood scripts, so it did not
  serve the domain. (Removed; `std::text::substr` is the operation, and the
  index brackets are gone.)
- **A call is a path call or a method call; there is no `f()()`.** `f(x)` calls
  the function named by a path, and `receiver.m(x)` calls a method. A function
  value is called through a name (`apply(x)`), never by calling a call result
  (`make()()`) or an index (`fs[0](x)`). (Dropped `Expression::Invoke`: an
  arbitrary callee adds no orchestration and reads badly.)
- **A closure literal is `[parameters] -> result? { body }`.** `fn name(...)`
  with an optional return type is a named function declaration, and
  `fn(types) -> result` is a function type; an expression-position `fn` is not
  a form. Brackets name a closure's parameters and nothing else, so the
  language still has no list literal and no index syntax. (The closure's
  parameters, return type, captures, and value type are unchanged.)
- **The sequence type is `Vec<T>`, and its element type is always written.** No
  bare `Vec`, and no implicit `Vec<txt>`. `Vec` is a declared `rec` in the
  `std::dynamic` namespace (`std/dynamic/vec.kiru`), not an intrinsic, and it
  has no literal and no index brackets: a value is built and changed through its
  `new`, `push`, `replace`, `at`, and `len` operations, so `use std::dynamic;`
  brings the type and its methods.
  (Renamed from `list`; the namespace was `std::vecs` while `vec` was a
  keyword.)
- **Generic type declarations:** `rec Name<A, B> { first: A, second: B };`
- **Generics stay; a container carries its element type.** `Vec<T>` and
  `Option<T>` keep their parameter, and the element type flows statically through
  `xs.at(i)`, `xs.push(v)`, `map`/`filter`/`sort_by`, and an `Option<T>`
  payload. The type system does not gain a dynamic element type. (Considered
  replacing the element type with one dynamic type; rejected — the syntax
  already answers "what is an element" by writing the type, and kiru is
  statically typed.)
- **Surface: `:` for types, `<>` for type parameters, `=` to assign a value.**
  `let p: Project = { name = "x" };`, `fn f(a: txt) -> txt`, `rec P { a: txt }`,
  and `Vec<txt>` for generics. Record literal fields keep `=`. A literal never
  names its type: a declaration defines a type and its shape, and every value is
  `let name: Type = shape;` (`let p: Project = { … };`).
- **`.` reads a member of a value or a type; `::` reads a namespace.** Field
  access (`r.a`), a method call (`xs.len()`), an associated-function call
  (`Vec.new()`), and variant construction and patterns (`Option.some`,
  `Shape.Circle(2)`) all use `.`; a namespace path (`std::fs::glob`) uses `::`.
  A variant is told from a field by the root: a type name is a variant, a value
  is a field. (Dropped `Name::Variant`: one separator for "a member of
  something", so `Option.some` mirrors `r.field`.)
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
- **A bound function is declared in an `impl` block and reached through its
  type.** `impl Type { fn ...; };` files every function inside under the block's
  type. A function whose first parameter accepts the receiver is a method,
  reached as a value call (`xs.len()`) or a `Type.name` call; its parameter's
  type is the dispatch target and its name is irrelevant. Every other function
  in the block is an associated function, reached as `Type.name(...)`
  (`Vec.new()`, `HashMap.new()`). A bound function is never a
  namespace-level name, so a `use` never shortens it and a bare name never
  reaches it. Its reachability is `pub` on the function and `pub` on the type,
  never a `use`. A name may repeat in a namespace only when the functions are
  bound to different types: `Vec.len` and `HashMap.len` share `len` in
  `std::dynamic`, and the receiver's type head selects between them. Every other
  declaration makes a name unique, so a genuine duplicate is an error. First
  user: the `std::process` command builder (`command(line).dir(...).run()`).
  Next: the vec/text helpers.

## Open questions

- **Exact operator spelling** for logic (`&&`/`||`/`!` vs `&`/`|`/`!`).
- **Fixed-size arrays:** out of scope for now, or a second sequence type?

## Phases (each reversible)

| Phase | Adds | Deletes / unlocks |
| --- | --- | --- |
| 0 ✅ | Precedence-climbing core (only `+` today) | — (pure refactor; `parse_sum` → `parse_binary`) |
| 1 ✅ | `bool` + `int`, literals, operators (`+ - * / % < <= > >= == != && || !`), typed predicates, `@()` formatting | `is_terminal`/`exists` → `bool`; `text`/`vec`/`path` predicates → `bool`; `+` concat dropped |
| 2 ✅ | nominal `rec` types with typed fields (2a); parameterized `Vec<T>` (2b); vec access (2c) | `std::get`/`std::text::substr` (removed); `len`/`memchr` → `int`; duplicate-field and `_`-ordering warts |
| 3 ✅ | generics (type params) + enums with payloads + variant/binding patterns + exhaustiveness | `Option` defined as a stdlib enum; no `Result` built in |
| 4 | closures/HOF (4b ✅ function values, 4c ✅ closures), the vec's `push`/`replace` operations (4a ✅) | `sort`/`map`/`filter` in kiru (the O(n log n) case) |
| 5 ✅ | shrink natives to the floor; document it (text kernel stays) | the budget below |

### Phase 2 breakdown

- **2a ✅ — nominal `rec` types.** `rec Name { field: Type, ... };` declares a
  type; annotations may name it (`: Name`); a record literal is checked against
  the expected type. Adds `Type::Named(TypeId)`; `TypeId` indexes a program type
  table. Field access has the field's type, and field assignment respects it.
  Duplicate fields always error. Record values hold `Value`s so a field can be
  any type.
- **2b ✅ — parameterized types.** `Type` carries arguments:
  `Type::Named(TypeId, Vec<Type>)`, so it is `Clone` but no longer `Copy`.
  `Vec<T>` is written in any annotation and the element type is always written.
  The compiler registers `Vec` as a declared `rec` in `std::dynamic`, so it is
  an ordinary declaration, not a runtime intrinsic. Runtime vecs hold `Value`s
  (`Value::Vec(Vec<Value>)`). The vec helpers (`push`/`replace`/`at`/`len`,
  `concat`/`reverse`/`contains`, `join`, `last`) are methods in
  `std::dynamic`.
- **2c ✅ — vec access, `len` → `int`.** `xs.at(i)` reads a vec element at an
  `int` index; an out-of-bounds index reports a message. `std::text::strlen`,
  `std::dynamic::len`, and `std::text::memchr` return `int`; `std::get` is
  removed. A vec is built and changed with `new`/`push`/`replace`.

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
  (`fn length<T>(xs: Vec<T>) -> int`): the call infers the type arguments by
  unifying each parameter type against its argument's type. `Type` gained
  `Named(TypeId, Vec<Type>)` and `Parameter(String)`; resolution takes a
  parameter scope; inference uses `unify`.
- **3c ✅ — stdlib `Option<T>`.** `enum Option<T> { some(T), none }` is defined
  in `std/std.kiru`, with `or`/`contains` methods. `std::text::memchr` returns
  `Option<int>` directly (the kernel is the absence itself, not a `-1`
  sentinel). `len` is the `std::dynamic::len` native, generic over the vec's
  element through the receiver. `require` flows an expected type into a match's
  arms, so a generic construction or a record literal can take its type from the
  match it sits in.

### Phase 4 breakdown

- **4a ✅ — the vec's `push` and `replace`.** A vec grows with
  `xs = xs.push(value)` and changes with `xs = xs.replace(i, value)`: in range
  `replace` replaces, and `push` appends (the `VecPush` and `VecReplace`
  natives mutate the register's vec in place). With in-place building, the vec
  helpers left the native kernel for kiru.
- **4b ✅ — function values + HOF.** Functions are values: a `fn(T) -> R` type,
  function-value expressions, calls through a function value, and generic
  call-site inference through a function argument. `map`/`filter` moved into
  `std/dynamic/vec.kiru`. With `<` extended to `txt`, `sort` moved too: it is
  an iterative O(n log n) bottom-up merge sort, and `sort_by` takes a
  comparator.
- **4c ✅ — closures.** An anonymous `[a: T] -> R { ... }` literal is a value.
  It captures the enclosing locals **by value** at creation (capture-all, so no
  free-variable analysis): the closure's leading registers are the captured
  values, `MakeFunction` snapshots them, and a captured local is read-only
  inside the closure. Nested closures capturing across a boundary work, a
  closure body is checked inline so a captured loop variable keeps its type, and
  `async` accepts a closure call. A closure with no return type is `[params]`
  and returns no value.

### Backlog — everything planned but not yet built

A durable backlog, so no plan is forgotten. Each item is filtered by the north
star: does it delete a native (or hold the surface flat), and does it make
orchestration programs smaller and clearer?

1. **3c ✅ — stdlib `Option<T>`.** `enum Option<T> { some(T), none }`; retype
   fallible helpers onto it (`std::text::memchr` returns `Option<int>`). `len` is
   the `std::dynamic::len` native.
2. **Bound functions ✅, and the stdlib is migrated.** A function in an `impl`
   block whose first parameter accepts the receiver is a method (its name is
   irrelevant; any file in a namespace); `receiver.name(args)` dispatches at link
   time on the receiver's declared type head, and a `Type.name(...)` call reaches
   an associated function. The stdlib now uses them: `std::process::command` is a
   builder; `Vec<T>` has `concat`/`reverse`/`contains`/`index_of`/
   `dedup`/`map`/`filter`/`first`/`last`/`sort_by`/`sort`/`join`; `txt` has
   `find`/`contains`/`trim`/`number`/`starts_with`/`ends_with`/`replace`/`lines`/
   `split`; `Option<T>` has `or`/`contains`. `Vec.sort` and `Vec.join` stay
   bound to `Vec<txt>` alone, since each applies to text elements rather than to
   every `Vec<T>`.
3. **Phase 4.** 4a ✅ the vec's `push`/`replace` operations. 4b ✅ function values
   + calls through a function value (`map`/`filter`/`sort`/`sort_by` moved
   into kiru; `<` extended to `txt`). 4c ✅ closures, capturing the enclosing
   locals by value (nested closures capture across a boundary).
4. **Phase 5 ✅ — the floor is documented, and revised.** The kernel is now the
   byte access `strlen`/`byte_at`/`memchr`/`substr`. `trim` and `split` left the
   kernel for kiru (they were Unicode policy — ASCII now, and a degenerate empty
   needle is an error), and `glob` left for kiru over the new `readdir` (it was a
   Rust crate, not a syscall). See the budget table below.
5. **Reconcile-later warts (deferred — each needs a syntax/design call).**
   (a) ✅ The literal never names its type: `Expression::Struct` (`Name { … }`)
   is gone, so a `{ … }` shape is typed by its annotation and the brace
   ambiguity is gone with it. (b) ✅ `rec` only declares; there is no anonymous
   record type, and the key-valued container is the ordinary `std::dynamic::HashMap`.
   (c) ✅ A generic
   literal cannot carry type arguments, which no longer arises: the literal
   carries no type at all.
6. **Small items.** A `let` element-type shorthand is deferred sugar
   (`let xs: Vec<int> = …` is the form today; the shorthand is unspecified).
7. **Open questions — settled.** Logic operators stay the symbols `&&`/`||`/`!`.
   Fixed-size arrays are out of scope; `Vec<T>` covers sequences. Recorded so the
   questions do not reopen.
8. **Argument-aware method dispatch (future).** Dispatch keys on the type head
   only, so a text-specific `sort`/`join` cannot be a `Vec<T>` method for every
   element type. Matching type arguments (most-specific impl) would let them
   apply to every `Vec<T>`.

### Deferred cleanups (from the backend audit)

- ✅ The stdlib `Nix` flag is now `bool`, through a declared `rec Project` in
  `main.kiru`; `wrap` matches `project.Nix` on `true`.
- ✅ Stale comments from earlier phases (`switch`, "defer", "internal number",
  "number kind", and the type-system "kind") are swept.

### Reconcile later (Rust warts to improve on)

- ✅ A record literal works as a `match` subject (Rust needs parens), and no
  literal names its type.
- ✅ `rec` declares a nominal shape; the key-valued container is the ordinary
  `std::dynamic::HashMap`, a declared `rec` the runtime keeps as a hash table.

## Intrinsic-surface budget

The native surface is the rows in `src/compiler/native_registry.rs`, tagged by
disposition. The floor is the OS boundary; the rest moves into kiru as the core
gains capability.

| Native | Role | Disposition |
| --- | --- | --- |
| `std::syscalls::posix_spawn` | `fork`/`exec` | floor |
| `std::syscalls::waitpid` | `waitpid` | floor |
| `std::syscalls::killpg` | `kill` | floor |
| `std::syscalls::write` | `write` | floor |
| `std::syscalls::isatty` | `isatty` | floor |
| `std::syscalls::read_stdin` | `read` | floor |
| `std::syscalls::unlink` | `unlink` | floor |
| `std::syscalls::access` | `access` | floor |
| `std::syscalls::mkstemp` | `mkstemp` | floor |
| `std::syscalls::readdir` | `getdents64` | floor (**added**) |
| `std::syscalls::nanosleep` | `nanosleep` | floor |
| `std::syscalls::getenv` | `getenv` | floor |
| `std::syscalls::getcwd` | `getcwd` | floor |
| `std::fs::read` | `open`/`read` | path convenience |
| `std::fs::write` | `open`/`write` | path convenience |
| `std::dynamic::Vec.new` | an empty vec | container |
| `std::dynamic::Vec.push` | append one element | container |
| `std::dynamic::Vec.replace` | replace one element | container |
| `std::dynamic::Vec.at` | read one element | container |
| `std::dynamic::Vec.len` | element count | container |
| `std::dynamic::HashMap.new` | an empty hash map | container |
| `std::dynamic::HashMap.insert` | write one key | container |
| `std::dynamic::HashMap.find` | read one key, as an option | container |
| `std::dynamic::HashMap.remove` | delete one key | container |
| `std::dynamic::HashMap.len` | entry count | container |
| `std::dynamic::HashMap.keys` | collect the keys | container |
| `std::dynamic::HashMap.values` | collect the values | container |
| `std::text::strlen` | byte length | kernel |
| `std::text::byte_at` | byte read | kernel |
| `std::text::memchr` | byte search | kernel |
| `std::text::substr` | byte range | kernel |
| ~~`std::fs::glob`~~ | a Rust crate | **deleted** — kiru over `readdir` |
| ~~`std::text::trim`~~ | Unicode policy | **deleted** — kiru over `byte_at`/`substr` |
| ~~`std::text::split`~~ | Unicode policy | **deleted** — kiru over `memchr`/`substr` |

Floor: 13 syscalls, plus the two `std::fs` path conveniences, the twelve
`std::dynamic` container operations, and the four-native byte kernel. A native
that touches the operating system lives in `std::syscalls` under its syscall or
C library name; a pure one stays with its kind. The rule: **a native earns its
place by being inexpressible in kiru, not by being convenient.** A high-level
convenience in the kernel — a `whole` native, say — is the drift to refuse: it
would make the next one look reasonable. The kernel is surfaced as methods
(`s.strlen()`, `s.byte_at(i)`, `s.substr(a, b)`, `s.memchr(needle)`), so the
everyday surface is uniform. (Moved into kiru earlier: `Vec.len` in 3c; `Vec.push` and
`Vec.replace` in 4a; `Vec.sort` in 4b, with `<` extended to `txt`; then the
whole `Vec`/`txt`/`Option` helper set became methods in the `impl` phase.)

### The minimization: which native deletes the most

Two additions delete three natives, and every deletion is one that was never a
syscall — which is the test the floor should have been applying all along.

1. **`std::text::substr(text, start, end) -> txt`** — the bytes in
   `[start, end)`. It is the one text operation kiru cannot express: building a
   substring with `@()` costs a rope node per piece, while a substr copies
   exactly its range. `memchr` stays, because a kiru byte loop would be O(n) VM
   dispatches where a native search is O(n) machine bytes — that one native is
   what keeps `split` linear. With `substr`, `trim` and `split` move to kiru:
   - `split(t, sep)` finds each separator with `memchr` in the O(1) remainder
     `t.substr(index, count)` and substrs the piece between — O(separators)
     native calls, linear overall, the same asymptotic cost as the native.
   - `trim(t)` walks `byte_at` from each end while it is ASCII whitespace, then
     substrs — O(the trimmed run) native calls, usually a handful.
   The behavior change is the point: Unicode whitespace and the char-boundary
   empty-separator split are Rust library *policy*, not the language. Text at the
   OS boundary is bytes, and an empty separator is an error. (Slicing as
   *syntax*, `xs[a..b]`, stays removed; `substr` is an ordinary `txt` method,
   because it is the operation that deletes two policy natives.)
2. **`std::syscalls::readdir(path) -> Vec<txt>`** — the entry names in a
   directory (`getdents64`). It deletes `glob`, which is a Rust crate, not a
   syscall — the clearest case of library logic above the floor. Glob becomes
   kiru: split the pattern on `/`, descend a literal segment directly, `readdir`
   and match a wildcard segment, `**` descends recursively, and the result is
   `sort`. It also makes a recursive walk, a directory tree, and a file picker
   expressible in kiru for the first time.

Candidates rejected, with the reason: a bulk `bytes(text) -> Vec<int>` would
delete `byte_at`/`memchr`/`trim`/`split` at once but pays an O(n) allocation on
every text operation (a 1 MB `contains` would build a million-element vec), so it
is slower, not equal; a unified `open`/`read`/`write`/`close` handle would
delete `std::fs::read`/`std::fs::write`/`read_stdin`/`write` but adds a resource
lifetime the language does not have (no ownership); a raw `syscall(nr, args)` is
not a language.

## Parser note

The expression parser is `parse_expression → parse_sum (+) → parse_postfix (.) →
parse_primary`. With a real operator set, replace `parse_sum` with a
precedence-climbing loop over a small `token → (precedence, associativity)`
table, keeping recursive descent for primaries and the postfix loop for `.` and
`()`. `<` is disambiguated by position: type-argument open in type positions,
comparison in expression positions.

- **Vec type syntax: `Vec<T>`.** The canonical vec type is `Vec<T>`, used in
  every position (`let tags: Vec<txt> = Vec.new();`, `fn f(tags: Vec<txt>)`).
  `Vec` is a declared `rec` in `std::dynamic`, so it has no literal and no
  brackets: a value is built with `Vec.new()` and grown with `push`. An
  element-type shorthand in `let` is deferred sugar, not a rule.
