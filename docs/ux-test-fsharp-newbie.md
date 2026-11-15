# UX test: an F# programmer tries som for the first time

Persona: I know F# (let-bindings, `printfn`, records, unions, `match … with`, pipes, `fun x -> …`, lists `[1; 2; 3]`, `Option`).
I didn't read the README or the std source. Everything below was learned from `som --help` and compiler errors.
Compiler: `som 0.1.0` (`target/debug/som`, branch `2027`), 2026-11-15.

Overall: **I got to a working program** (enum + struct + `impl` + `match` + loop + mutation) in about 75 compiles.
The good errors are really good: optionals, exhaustiveness, visibility, use-after-move, the empty-file welcome.
The bad ones come from **F# habits the parser doesn't recognise**. They either say "expected X" or suggest a "fix" that makes the code worse.

---

## 1. Getting started

```
$ som
The som compiler
Usage: som <COMMAND>
Commands: run, build, check, lsp, help
A <PATH> is a .som file, or a directory of them.
```

👍 Clear. I know to do `som run file.som`.

### Hello world (5 attempts)

| Attempt | Result |
|---|---|
| `printfn "Hello, world!"` | `error: expected ;` pointing at the string, "expected a semicolon after the expression" |
| `printfn "Hello, world!";` | same error 🤔 I *added* a semicolon. |
| `printfn("Hello, world!");` | `cannot find function printfn` (no hint) |
| `print(...)` / `println(...)` | ✅ `println is defined in module som::io … try som::io::println or use som::io;`, with both patches shown |
| `use som::io;` + `println("Hello, world!");` | ✅ prints |

> Thoughts: the first error confused me. It wanted a semicolon *before* my string. I didn't realise function calls need parentheses. A hint like
> "som calls functions with parentheses: `printfn("…")`" would have saved two tries.
> `printfn` → no hint, but `println` → great hint. Suggesting `println` for `printfn` (edit distance 1) would be the obvious win.
> `Console.WriteLine` → "cannot find variable `Console`". Fair enough.

Funny detail found at the very end: running an **empty file** gives a lovely "welcome to som!" message with a hello-world patch.
If I'd started with an empty file I'd have skipped all of the above.

## 2. Printing numbers

```som
use som::io;
let y = 7;
println(y);
```
```
error: expected `cstr`, found an integer
   4 │ println(y);
     │         ━ this is an integer
   3 │ pub fn println(text: cstr) {
     │                      ──── `cstr` expected because of this
   4 │     c::printf("%s\n", text);      (std/io/io.som)
```

> Thoughts: I wasn't supposed to read std, but the error showed me std. And that taught me **`c::printf` exists**!
> As an F# person I know `printf "%d"`, so `c::printf("%d\n", y);` worked straight away. That's how I printed everything from then on.

Things that didn't work: `string y` (suggests `println(string, y)` 🙃), `y.to_string()`, `y.ToString()` (both "integer has no method").
I never found a som-native way to turn a number into text.

## 3. Functions

| Attempt | Error / suggestion |
|---|---|
| `let add a b = a + b` | `missing =` → suggests **`let add = a b = a + b`** ❌ |
| following that suggestion | `expected ;` → suggests **`let add = a; b = a + b;`** ❌ |
| `let add = fun a b -> a + b;` | suggests **`let add = fun; a b -> a + b;`** ❌ |
| `let add = \|a, b\| a + b;` | `expected expression` |
| `fn add(a, b) { a + b }` | ✅ works, and **parameters don't need types!** |

> Thoughts: the "add the missing X" patches are the worst part of the experience. They are mechanically correct for the parser and nonsense to a human.
> I followed one and ended up in a loop of worse code. I only found `fn` because an earlier error had shown me `pub fn println(text: cstr) {`.
> Suggestion: when `let NAME IDENT…` is followed by `=`, say "functions are declared with `fn name(a, b) { … }`".

Untyped params + inference felt very F#-y. 👍

## 4. `if`, comparison, recursion

```som
fn fact(n) { if n = 0 then 1 else n * fact(n - 1) }
```
→ `missing { … expected an opening brace for the if body` (points at `=`).

With `==` but `then`: same error at `then`. With braces but `n = 0`:

```
error: `n` is not mutable
     ▪ make `n` mutable:  fn fact(mut n) {
error: expected `bool`, found `()`
```

> Thoughts: **the #1 F# trap.** In F#, `=` *is* equality. The compiler's top fix is "make `n` mutable", which would compile and be wrong!
> Please special-case "`=` inside an `if`/`while` condition" → "did you mean `==`?".
> `n <> 0` → "expected an expression". A hint pointing to `!=` would help.

`if n == 0 { 1 } else { n * fact(n - 1) }` → ✅ 120.

## 5. Mutation and loops

- `let mutable total = 0;` → suggests `let mutable = total = 0;` ❌ (again)
- `let mut total = 0;` ✅ (guessed from the "make `n` mutable" patch earlier)
- `for i in 1 .. 10 do` → "expected an opening brace for the for body" 👍 clear
- `total <- total + i` inside braces:

```
error: this value is unused
   3 │     total <- total + i
     │     ━━━━━━━━━━━━━━━━━━ working it out does nothing else
     ┆ use it, or if dropping it is the point, `drop(…)` it
```

> Thoughts: 😵 It parsed `total <- total + i` as `total < -(total + i)`. "Working it out does nothing else" means nothing to me.
> F# people *will* write `<-`. Please detect `x <- …` and suggest `x = …`.

- `total = total + i;` ✅ — but printed **45**, not 55.
  **`1 .. 10` is exclusive in som; in F# it's inclusive.** No warning, silently different answer.
  Tried `1..=10` → "expected an expression". I fixed it with `1..11`.
- `total += i;` → "expected an expression". No compound assignment, and no hint that it's missing.
- `while j < 3 { … }` ✅

## 6. Types

`let n: int = 3;` → `cannot find type int` with no suggestion. I probed by hand:

| works | doesn't exist |
|---|---|
| `i32`, `i64`, `f64`, `bool`, `char`, `str`, `cstr`, `usize` | `int`, `int32`, `Int`, `float`, `double`, `string`, `String` |

> A "did you mean `i32`?" for `int`/`int32`, `f64` for `float`/`double`, and `str` for `string` would be cheap and save a lot.

Two string types. `let s: str = "hi"; println(s);` →
`C can't take a str … C needs text ending in a zero byte`. 🤔 I called `println`, not C.
It's understandable once you've seen that `println` wraps `printf`, but it's confusing for a newbie.

## 7. Arrays / lists

- `[1; 2; 3]` (F# list) → "missing `]`", suggests `[1; 2]; 3];` ❌. It seems `[x; n]` means "repeat"? Never explained.
- `[1, 2, 3]` ✅, `for x in xs` ✅
- `xs.len()` → "`[i32; 3]` has no method `len` — **it's a property**, read it without parentheses" 👍 great
- `xs[5]` on a 3-array → compile-time "index 5 is out of bounds, the array has 3 elements" 👍
- `xs[i]` with variable `i` → "an index has to be a number written out … use `xs.get(i)`, which gives an optional". Strict but well explained.
- `xs.push(4)` → no method. Couldn't find any growable list: `List::new`, `Vec::new`, `ResizeArray`, `HashMap::new`, `Map::new`, `som::collections::…` all just say "cannot find function". `use som::*;` isn't allowed.
  **There's no way to discover what std contains from the compiler.** E.g. "`som::foo` doesn't exist; `som` has: io, …" would help.
- `xs |> List.map (fun x -> x * 2)` → three cascading errors with silly patches (`fun, x`, `fun x)`). No pipes, no lambdas as far as I can tell.

## 8. Records → structs

- `type Person = { Name: cstr; Age: i32 }` → `expected one of i32, bool, str, identifier, &, fn, found {`. So `type` is an alias. Plausible.
- `struct Person { Name: cstr; Age: i32 }` → suggests `struct Person { Name: cstr}; Age: i32 }` ❌. Should say "fields are separated by `,`".
- `struct Person { Name: cstr, Age: i32 }` ✅
- `Person { Name = "Ann"; Age = 30 }` (F# record syntax) → suggests **`let p = Person; { Name = "Ann"; Age = 30 };`** ❌❌. This one is actively harmful.
- `Person { Name: "Ann", Age: 30 }` ✅ (guessed from Rust/JS knowledge, not from the error)

## 9. Unions → enums + match

- `type Shape = | Circle of f64 | …` → "expected a type". No hint that this is `enum`.
- `enum Shape { Circle(f64), Square(f64) }` ✅ (guessed)
- `match s with | Circle r -> …` → "expected the match arms" at `with` 👍 ok
- `| Circle r ->` inside braces → "expected a pattern"
- `Circle(r) -> …,` → "missing `=>`" 👍 **this taught me `=>`**. But the patch inserts `=>` *before* `->` (`Circle(r) => -> …`) instead of replacing it.
- Calling `area(Circle(2.0))` → `cannot find function Circle`. No hint that it's `Shape::Circle`. Tried `Shape.Circle` → "cannot find variable Shape".
- `area(Shape::Circle(2.0))` with an **untyped** `fn area(s)`:
  ```
  error: `Circle(r)` can't match a an unknown type
  ```
  🐛 typo "a an". Also inference doesn't use the constructor patterns to work out `s: Shape`, even with `Shape::Circle(r)` patterns. And it doesn't suggest "annotate `s`".
- `fn area(s: Shape)` ✅ 12.56. Unqualified `Circle(r)` patterns work once the type is known.

**Exhaustiveness check is excellent:**
```
error: `Tri(_, _)` isn't matched
   3 │     match s {
     │           ━ this `Shape` can be that
     ▪ add an arm for `Tri(_, _)`
```
Small issues: the suggested arm is `Tri(_, _) => {},`, which returns `()` where `f64` is expected. The diff rendering also merges two lines into one, with no line numbers.

## 10. Tuples

`(a / b, a % b)`, `let (q, r) = …`, `match (i % 3, i % 5) { (0, 0) => … }`: all fail with "missing `)`" and suggest `(a / b), a % b)` ❌.
> If tuples aren't in the language, saying so ("som has no tuples; use a struct") beats a broken patch. FizzBuzz-by-tuple-match is a very F# thing to try.

## 11. Option → `T?`

- `None` → "**som writes `None` as `none`**" 👍👍 lovely
- `Some(x)` / `some(x)` → "cannot find function". No hint that a plain value is auto-wrapped.
- `-> Option<i32>` → "cannot find type `Option`" (no hint). `-> i32?` ✅ (guess).
- Passing `i32?` to an `i32` param:
  ```
  error: expected `i32`, found `i32?`
     ┆ check it first: inside `if v { … }`, it holds one
     ┆ or give a fallback for `none`: `v else …`
     ▪ give a fallback for `none`: `v else 0`
  ```
  👍👍 **Best error of the session.** It taught me two idioms at once, and both worked right away.
- ⚠️ But passing a `none` `i32?` to `c::printf("%d")` **compiles and prints `0`**. Varargs C calls skip the check entirely.

## 12. Methods

- F# `with member this.Sum() = …` → two unhelpful "expected ;" errors.
- `fn sum(self)` inside the struct → "`self` only exists in methods — a method is written in an `impl` or `trait` block" 👍 taught me `impl`.
- `impl Point { fn move_by(self, dx) { self.x = … } }` → "`self` is not mutable", suggests `mut self`.
  I accepted. → "`q` is used after it was consumed".
  The hint says "borrow it there: `&q`". I wrote `(&q).move_by(10)` → "the method takes the Point itself … have the method take `self: &…` instead".
  → `self: &Point` → "cannot assign through a `&` reference … needs `&mut`" → `self: &mut Point` ✅

> Thoughts: each step was individually reasonable, and I got there in 4 hops. But the **first suggestion sent me the wrong way**.
> When the body of a method mutates `self`, `self: &mut Point` is almost always what's wanted. Moving `self` is almost never what's wanted.

- Calling a `fn sum(self)` getter twice → "`p` is used after it was consumed". For an F# dev, *reading* a record consuming it is very surprising.
  The hint ("borrow it there: `&p`") doesn't work for a method receiver, as we saw above. It should point at the method signature instead.
- `Point::origin()` static functions ✅

## 13. Higher-order functions — couldn't do it

```som
fn twice(f: fn(i32) -> i32, x: i32) -> i32 { f(f(x)) }
```
```
error: cannot find function `f`
     │ ━ not found in this scope
```
> 🤨 `f` is a parameter, so it *is* in scope. `(f)(x)` gives the same error. `f.call(x)` → "`fn(i32) -> i32` has no method `call`".
> The type `fn(i32) -> i32` parses, so I expected to be able to call it. Anonymous `fn(x: i32) -> i32 { … }` → "expected expression".
> As an F# programmer this is the biggest missing piece. If it's not supported yet, the error should say so instead of "not in scope".

Generics: `fn id<T>(x: T) -> T { x }` ✅

## 14. Strings

- `s.len` ✅ (bytes: `"héllo".len` = 6)
- `s.Length` → "**`str` is not a struct — only structs have fields and methods**". That contradicts `s.len` working one line earlier.
- `s.concat(…)` → no method, and no list of methods that *do* exist.
- `s[0]` → "cannot index `str` … use `s.get(0)`" 👍. Returns a byte (195 for `é`).
- `"Hello " + name` → "cannot apply `+` to a string and a string". No concat at all?
- `a == b` on `str` or `cstr` → "no such operation — **to give it `==`, implement `Eq` for the type**". I can't implement a trait for a builtin, so the advice can't be followed. I found **no way to compare two strings**.
- `$"Hello {name}"` → "expected expression".

## 15. Numbers

- `x / 2.0` with `x: i32` → "cannot apply `/` to `i32` and a float". No suggestion. Tried `float x`, `f64(x)` ("this is a type, where a value goes"), `x.to_f64()`. **`x as f64` works**, but no error ever mentioned `as`.
- `let n: f64 = 3;` → error (F# also rejects that, fine).
- 🐛 **Division by zero is undefined behaviour**: `fn div(a, b) { a / b }` then `div(1, 0)` prints `-85999232` and exits 0.
  .NET throws `DivideByZeroException`. Given how strict array indexing is, this is surprising.
- Overflow wraps silently (`100000 * 100000` → `1410065408`). F# unchecked does the same, so fine.

## 16. Program structure & exit codes

- Top-level statements work without `main` 👍 (like an F# script).
- 😲 **The last top-level expression becomes the exit code.** `c::printf("hi\n")` without `;` → exit code **3** (printf's return value). A `match` whose arms call printf → exit 2. A lone `42` → exit 42.
  Clever, but I'd never have guessed it, and the shell reporting my program as failed is alarming.
- `fn main() {…}` + top-level code → "`main` is already defined with the same signature … defined again here", pointing at a `printf` call.
  It needs a note that top-level code *is* the implicit `main`.
- `fn main(argv)` → "cannot infer the type of this expression", reported **twice** (identical duplicate).
- `fn main(argv: [str])` → "a slice is borrowed — write it `&[T]`" 👍
- 🐛 **`fn main(argv: &[str]) -> i32` passes all checks and then the C compiler fails:**
  ```
  error: the C compiler rejected the generated code
       ┆ this is a bug in the som compiler
       ! error: too few arguments to function call, single argument 'argv' was not specified
       !    return (int)som_main();
  ```
  I never found out how to read command-line arguments.

## 17. Multiple files

- `proj/main.som` + `proj/mathy.som`, calling `mathy::square` → "cannot find function" (no hint).
- Calling bare `square` →
  ```
  error: `square` is private to `mathy.som`
     │    ────── defined here, without `pub` or `mod`
     ▪ make it `mod`
  ```
  👍 Taught me that files in one directory form one module, items are file-private by default, and `mod fn` shares them.
- A `mathy/` subdirectory with `pub fn` → still "cannot find function `mathy::square`". It only worked after I guessed `use mathy;`.
  The `print` error earlier suggested `use som::io;`, so I'd expect the same hint here.

## 18. CLI

- `som runn` → clap's "a similar subcommand exists: 'run'" 👍
- `som run nope.som` → "create `nope.som`, or a directory `nope/` with a `main.som`" 👍
- `som run hello.txt` → "create **`hello.som`**". Slightly odd. It should mention that som files end in `.som`.
- Empty file → 🐛 "the program has **an no** entrypoint " (typo + trailing space), followed by a really nice welcome/hello-world patch.
- `som build x.som` → `./x` executable, clean. Status lines (`built: …`) go to stderr, so program output stays clean 👍
- `som check` prints `checked: file` 👍

## 19. Capstone — worked on the first try

```som
use som::io;

enum Tx { Deposit(i32), Withdraw(i32), Fee }
struct Account { owner: cstr, balance: i32 }

impl Account {
    fn apply(self: &mut Account, tx: Tx) {
        match tx {
            Deposit(n) => self.balance = self.balance + n,
            Withdraw(n) => {
                if n > self.balance { println("insufficient funds!"); }
                else { self.balance = self.balance - n; }
            },
            Fee => self.balance = self.balance - 1,
        }
    }
}

let mut acc = Account { owner: "Ann", balance: 0 };
let txs = [Tx::Deposit(100), Tx::Withdraw(30), Tx::Fee, Tx::Withdraw(500)];
for tx in txs { acc.apply(tx); }
c::printf("%s has %d\n", acc.owner, acc.balance);
```
```
insufficient funds!
Ann has 69
```

---

## Summary

### Bugs
1. `fn main(argv: &[str])` passes all checks, then generates C that doesn't compile (`som_main()` called with no args).
2. Division by zero is C undefined behaviour: garbage result, exit 0.
3. Calling a function-typed parameter (`f: fn(i32) -> i32`, then `f(x)`) → "cannot find function `f` … not found in this scope".
4. Message typos: "can't match **a an** unknown type", "has **an no** entrypoint " (with a trailing space).
5. `main(argv)` "cannot infer" error is emitted twice.
6. `none`-valued `i32?` passed to `c::printf` compiles and prints 0.

### Misleading suggestions (highest impact for newcomers)
| Input | Suggested "fix" |
|---|---|
| `let add a b = …` | `let add = a b = …` |
| `let mutable x = 0` | `let mutable = x = 0` |
| `Person { Name = "Ann"; … }` | `let p = Person; { … }` |
| `struct P { a: i32; b: i32 }` | `struct P { a: i32}; b: i32 }` |
| `[1; 2; 3]` | `[1; 2]; 3]` |
| `(a / b, a % b)` | `(a / b), a % b)` |
| `string y` | `string, y` |
| `if n = 0 {` | make `n` mutable |
| `fn m(self) { self.x = … }` | `mut self` (should be `self: &mut T`) |
| `Circle(r) -> …` | `Circle(r) => -> …` |
| `cstr == cstr` | implement `Eq` for the type |

Most of these are "insert the expected token at the error position". When the patch can't produce valid code, it's better to show no patch at all.

#### Re-test (same day, after a compiler rebuild)

| Input | Now |
|---|---|
| `Person { Name = "Ann"; … }` | ✅ no patch. Message is still "expected `;` at the end of the variable declaration" |
| `(a / b, a % b)` / `match (x, y)` | ✅ no patch. Still just "missing `)`", with no word about tuples |
| `let add = fun a b -> …` | ✅ no patch (it used to suggest `fun; a b`) |
| `let add a b = …` | ❌ still `let add = a b = …`. Span moved to `a` |
| `let mutable x = 0` | ❌ still `let mutable = x = 0`. Span now covers `total` |
| `Circle(r) -> …` | ❌ span now covers `->`, but the patch still gives `=> ->` instead of replacing it |
| `struct P { a: i32; b: i32 }` | ❌ unchanged: `{ a: i32}; b: i32 }` |
| `[1; 2; 3]` | ❌ unchanged: `[1; 2]; 3]` |
| `string y` | ❌ unchanged: `string, y` |
| `xs \|> List.map (fun x -> …)` | ❌ unchanged: `fun, x` and `fun x) ->` |
| `if n = 0 {` | ❌ unchanged: make `n` mutable |
| `fn m(self) { self.x = … }` | ❌ unchanged: `mut self` |
| `cstr == cstr` | ❌ unchanged: implement `Eq` |
| `printfn "…";` | unchanged (no patch; "expected ;" at the string) |

### Cheap wins for F# (and ML-family) people
- `=` in a condition → "did you mean `==`?"; `<>` → `!=`; `x <- e` → `x = e`.
- `1 .. 10` being exclusive — warn, or mention it in some error. It silently gave 45 instead of 55.
- `let f a b = …` → "functions are `fn f(a, b) { … }`". `fun x -> …` / `|x| …` → say whether lambdas exist.
- Type-name suggestions: `int` → `i32`, `float`/`double` → `f64`, `string` → `str`, `Option<T>` → `T?`.
- `printfn` → "did you mean `println`?" (it already works for `print` → `som::io::print`).
- Unknown enum constructor `Circle(…)` → "did you mean `Shape::Circle`?".
- "No such operation" for numeric types → mention `as f64`.
- Untyped param that can't be inferred from patterns → suggest an annotation.
- Some way to discover std from the compiler (list modules/methods when something isn't found).

### What felt great
- The optional (`T?`) error: it teaches both `if v {}` narrowing and `v else 0`.
- `None` → "som writes `None` as `none`".
- Exhaustiveness checking with the missing pattern spelled out.
- Visibility errors (`private to mathy.som … make it mod`).
- Use-after-move errors with both the "consumed here" and "used here" spans.
- `len()` → "it's a property".
- Compile-time out-of-bounds, and `.get(i)` hints for dynamic indexing and strings.
- The empty-file welcome message.
- Untyped `fn add(a, b)` with inference, plus generics that just work.
- Clean CLI: clap typo hints, status on stderr.
