---
title: Return and Void
description: The return rule, void functions, and why there is no recursion.
---

`return(expr);` ends the function with that value. It uses the same
parentheses as a call:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn greeting(txt name) {
  return("hello, " + name);
};
```

`greeting` evaluates the concatenation and returns it as the function's
result.

## The Return Rule

- `return(...)` is optional.
- When present, it may only be the last statement of a function declaration.
- It may not stand inside a `switch`, a `case`, a `default`, a `defer`, or
  any other nested block.
- The returned value must be text or record.
- `main` is void and must not contain `return` at all.

A return that is not the last statement is an error:

```console
$ kc main.kiru
main.kiru:2:3: error: `f` may only `return` as its last statement
  return("x");
  ^^^^^^^^^^^^
```

`main` declares no value, so a return in it is rejected:

```console
$ kc main.kiru
main.kiru:2:3: error: `main` must not contain `return`
  return("x");
  ^^^^^^^^^^^^
```

A return that sits in a nested block is also an error. The workaround is to
assign the result and return once at the end:

<span class="filename">Filename: src/main.kiru</span>

```kiru
fn pick(txt s) {
  txt result = "";
  switch(s) {
    case("a") { result = "first"; };
    default { result = "other"; };
  };
  return(result);
};

fn main() {
  std::print(pick("a"));
};
```

`pick` assigns inside the arm and has a single return, so the program
prints `first`.

## A Function Without return Is Void

When a function has no `return`, it is void, and its call has the `nothing`
kind. A void call may only be a statement, or the invocation `std::async`
spawns. Binding, passing, storing, or returning it is a compile error:

```console
$ kc main.kiru
main.kiru:2:21: error: expected text, found nothing
fn main() { txt x = note("hi"); };
                    ^^^^^^^^^^
```

```console
$ kc main.kiru
main.kiru:2:24: error: expected text, found nothing
fn main() { std::print(note("hi")); };
                       ^^^^^^^^^^
```

A `return` of a void call is rejected because the returned value must be
text or record:

```console
$ kc main.kiru
main.kiru:2:17: error: expected text or record, found nothing
fn f() { return(note("hi")); };
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
