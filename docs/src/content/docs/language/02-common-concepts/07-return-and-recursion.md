---
title: Return and Void
description: The return rule, void functions, and why there is no recursion.
---

A function ends when its body ends. `return` is an early exit: `return expr;`
ends the function with text or record, and `return;` ends a void function
with no value.

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn greeting(txt name) {
  return "hello, " + name;
};
```

`greeting` evaluates the concatenation and returns it as the function's
result.

## The Return Rule

- `return` is optional.
- It may appear anywhere, including inside a `switch` arm, except inside
  `defer`.
- Every value return in one function carries the same kind, and that kind is
  the function's kind.
- A function with a value return must not fall through: every path must end in
  `return`, `panic`, or a `switch` with a `default` whose every arm ends in a
  terminator. Statements after a terminator are unreachable and are allowed.
- The returned value must be text or record.
- `main` is void for every caller, so it may `return;` or `return expr;` and
  a returned value is discarded.

An early return in a switch arm is allowed, and the final return settles the
kind:

```kiru
fn pick(txt s) {
  switch(s) {
    case("a") { return "first"; };
    default {};
  };
  return "other";
};
```

A value function that can fall through is an error:

```console
$ kc main.kiru
main.kiru:1:4: error: `pick` returns text but can fall through; every path must end in `return` or `panic`
fn pick(txt s) { switch(s) { case("a") { return "first"; }; }; };
   ^^^^
```

## A Function Without return Is Void

When a function has no value return, it is void, and its call has the
`nothing` kind. A void call may only be a statement, or the invocation
`std::async` spawns. Binding, passing, storing, or returning it is a compile
error:

```console
$ kc main.kiru
main.kiru:2:21: error: expected text, found nothing
fn main() { txt x = note("hi"); };
                    ^^^^^^^^^^
```

A `return` of a void call is rejected because the returned value must be
text or record:

```console
$ kc main.kiru
main.kiru:2:17: error: expected text or record, found nothing
fn f() { return note("hi"); };
                ^^^^^^^^^^
```

A void function that writes a line is a common shape:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn note(txt message) {
  std::print(message);
};

fn main() {
  note("hello");
};
```

## panic

`panic;` is a keyword statement that ends the run. It is a terminator, so a
value function may end in it, and it is allowed inside `defer` where `return`
is not.

## No Recursion

A function cannot reference itself, and a forward reference is an error, so
there is no recursion anywhere. A call to the function's own name is
rejected:

```console
$ kc main.kiru
main.kiru:1:13: error: a function cannot reference itself
fn loop() { loop(); };
            ^^^^
```

Calling a function before it is declared is also rejected:

```console
$ kc main.kiru
main.kiru:1:13: error: `later` is declared after this point
fn main() { later(); };
            ^^^^^
```

## Return Values Are Checked

The value returned must be text or record, and a value of any other kind is
rejected where it is returned. There is no inferred return kind: a function
that returns is text or record because of the expression it returns. [Type
checking](/language/04-types/03-type-checking/) shows the rule.
