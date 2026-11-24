# Errors

*Status (2026-11-24): `T!`, `fail`, `?`, `else`, `else e =>`, `fail …` arms in a `match`,
`fn main() -> !`, error sets worked out across the program and checked where they're
declared, and `som api`. Tests: `tests/corpus/failing`.*

A function that can fail gives a `T!`: a `T`, or why there isn't one. Why is any value
shown as text, any type that implements `Display`. No one has to agree on one error type:
what each function fails with is worked out from its body, so failures from different
libraries just add up, with nothing to convert.

```som
fn read(path: string) -> string! {
    if !File::exists(path) {
        fail NotFound { path };
    }
    …
}

fn load(path: string) -> Config! {
    let text = read(path)?;                              // passes a failure on
    let port = parse(text) else fail "reading the port"; // says what was being done
    Config { port }
}

fn main() -> ! {
    let config = load("app.conf")?;
    …
}
```

When `main` fails, the program prints `error: reading the port: not a digit at 3` and
exits with 1.

## What's what

- **`T!`** is a `T`, or a failure. `!` alone is `()!`, for a function that gives nothing.
  `T?!` fails, or gives a `T` or `none`: a lookup that can fail, where not finding is no
  failure. (`T!?` isn't useful, and `?`, `else` and patterns take one layer off at a time,
  outermost first.)
- **`fail value`** gives up: the function fails with `value`, of any type that's
  `Display`. It's an expression that never gives a value, like `return`. `fail` only fits in
  a function giving a `T!`.
- **`value?`** gives what a `T!` holds, or the function fails with the same failure.
- **`value else fallback`** gives what it holds, or `fallback` (which can also leave:
  `else return 0`, `else fail …`).
- **`value else e => …`** names why it failed: `e` is an `Error`, shown as the whole story.
- **A `T` where a `T!` is expected is held by it**: `5` in `fn f() -> i32! { 5 }`, and a
  `-> !` function that gets to its end, or a bare `return;`, gives `()`.

A `T!` can't be used as its `T`, and dropping one on the floor (`save(path);`) is an error:
a failure would go unseen. Both errors suggest `?`.

## Context: `else fail "…"`

In the `else` of a failure with no name, `fail` keeps that failure:

- `fail "reading the port"`, text, says what was being done. It's the same failure, with
  a line more, shown in front: `reading the port: not a digit at 3`. Matching on it still
  finds the `ParseError` underneath, and so does working out what it fails with.
- `fail Other { … }`, anything else, fails in its place, and keeps it as the cause:
  shown `other: not a digit at 3`; it fails with an `Other` now.

With a name, `else e => …`, it's up to the code: `fail e` passes it on as it is.

## Matching failures

```som
match load(path) {
    config => use(config),
    fail NotFound { path } => println("no {path}"),
    fail ParseError::BadDigit { at } => println("bad digit at {at}"),
    fail e => println("{e}"),
}
```

Patterns other than `fail …` match the value it holds (and `_` anything). `fail e` takes
any failure, as an `Error`. `fail` with a struct's or an enum's pattern takes a failure
with a value of that type, matched through a reference, so its fields are lent (copied, if
they copy). Struct patterns, `NotFound { path }`, work anywhere now, not only after `fail`.

## What a function fails with

Worked out after type checking (`crates/compiler/4-typeck/src/failures.rs`), to a fixed
point over the program: a function fails with the types of the values its `fail`s are
given, and with what the `T!`s it passes on with `?` fail with. A struct or enum is one
member: a function that fails with `ParseError::Empty` fails with `ParseError`. Text is a
`string`. Context (`else fail "…"`) adds nothing of its own.

Where it can't be known, it's "anything" (written `…`): through a function's address or a
closure, a `T!` passed in as a parameter or kept in a field, a trait's method in generic
code, and values of a type that isn't a struct or enum (`fail 5`).

With that known:

- A `match` on a `T!` with an arm for every type it can fail with needs no `fail e`. One
  that can fail with anything needs it. Missing arms are named: `fail Bad::Late`.
- A `fail` arm for a type it can't fail with is an error (`CantFailWith`).

### Saying it

A function can say what it fails with: `-> i64!ParseError`, `-> Config!(IoError,
ParseError)`, `-> !()` for nothing. Callers then see that and nothing else, and its body is
held to it: failing with anything else, or passing on what can, is an error at that line
(`FailsUndeclared`), fixed by handling it there, wrapping it in one of the declared types,
or adding it. Without that, everything is worked out; saying it is for promises.

### `som api`

```
$ som api src
pub fn parse(text: string) -> i64!ParseError
pub fn load(path: string) -> i64!(NotFound, ParseError)
impl Config {
    pub fn open(path: string) -> Config!(NotFound, ParseError)
}
$ som api src > api.txt
$ som api src --check api.txt     # in CI: fails, saying what changed
```

What a program or library lets others use, one line each, with what each `T!` fails with
spelled out, worked out or not. Kept in the repository, a change deep inside that makes a
public function fail with something new shows in review, though no signature in the diff
changed. The language server shows the same in its signatures.

## How it's built

- **`T!` is `Outcome<T>`** (`std/error.som`), an ordinary enum, `Ok(T)` or
  `Failed(Error)`, so moves, drops, borrows and code generation needed nothing new.
- **`Error`** holds what it failed with as a `dyn Display` ([dyn.md](dyn.md)), with the
  context lines and the cause. A `fail NotFound { .. }` arm tests which type the `dyn`
  holds. `error_of`, `error_context` and `error_caused` are what the compiler calls;
  `main_exit` is what C's `main` calls for a `fn main() -> !`.
- **Sets are only in the checker.** At run time any failure is an `Error`; passing one on
  converts nothing.

## Not yet

- What closures fail with, and generic code over what its function parameters fail with:
  both are "anything" for now.
- A `fail` arm for a generic error type, `fail Wrapper<i32> { .. }`.
- Taking the value out of a failure in a `fail` arm, rather than lending it.
- `e.is<T>()` and friends on `Error`, or on any `dyn`: method calls with type arguments
  don't parse yet.
- Writing `Display` for an error type is two members (`display_len`, `display`); a
  shorthand for "shown as this string" would help.
