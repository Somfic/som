# som UX test: a JavaScript dev's first hour

**Setup:** I'm playing a fairly naive programmer who only knows JavaScript. I didn't read the readme or the std source. Everything I learned came from `som --help` and compiler errors (std snippets that errors printed count as fair game).

- Binary: `target/debug/som` (built with `cargo build -p som-cli`)
- Test files: `/private/tmp/claude-501/-Users-lucas-som/2b6938b8-e6de-4d78-a501-81c3638fa1e1/scratchpad/play/` (numbered `01.som` … `141.som`)
- All runs used `NO_COLOR=1`

---

## TL;DR

**What felt great**
- The "not mutable" (`let mut`), "used after it was consumed", "might be `none`" and typo ("did you mean `println`?") errors read like a patient teacher. Each one says what's wrong, where, *and* gives a patch.
- "`print` is defined in module `som::io`… add `use som::io;`" got me from zero to hello world in 3 tries.
- Untyped `fn add(a, b) { a + b }` with inference feels like JS.
- Same-directory files share a namespace, so no import ceremony. `som run .` just works.
- When a type error is about a std function, it shows the std signature, so I learned syntax (`fn`, `name: type`, `struct`, `impl`, `-> Self`) from it.

**Bugs I hit (should fix)**
1. **The compiler hangs forever** on `f(a.i32);` or `f(a.cstr());`, i.e. a keyword after `.` inside `(...)` or `[...]`. There's no output and it never exits. I hit this naturally while guessing `myString.cstr()`.
2. **Internal compiler error:** a file whose last line is a bare string expression (`"hello"`) produces C that clang rejects.
3. **`List::from([1,2,3])` is always ambiguous.** The hint "specify a return type" can't help, because both overloads return `List<T>`. I added `: List<i32>` and got the exact same error.
4. **Assigning a field of a moved value compiles:** `let b = a; a.x = 5;` passes `som check`.
5. `som check` on 120.som printed the **same error twice**.

**Footguns that silently produce wrong output**
- `c::printf` has no format/argument checking. `printf("%d", "hello")` and `printf("%d", someRef)` print garbage pointers. Optionals and references can be passed straight into varargs.
- `divide(10, 0)` prints a random number and **exits 0**. `i32::MAX + 1` wraps silently.
- **The last top-level expression becomes the exit code.** A trailing `c::printf("hi\n")` without `;` exits with **3**, and a `match` whose arms return printf's result exited with **6**.

**Biggest learning-curve walls (no help from the compiler)**
- Printing anything that isn't a literal: no string concat on literals, no interpolation, no `format`, `println` only takes `cstr`, and I never found a way to print a `String`.
- Three string types (`str`, `cstr`, `String`), with error prose calling all of them "a string".
- JS keywords (`const`, `var`, `function`, `class`, `switch`, `throw`, `import`, `===`, `+=`, `++`, arrow functions) all give the generic "expected ;" or "expected expression", with no "som spells this X" hint.
- The error for a method that doesn't exist never lists the methods that do (the field error does list fields).

---

## The session

### 1. Finding the CLI

```
$ som
The som compiler
Usage: som <COMMAND>
Commands: run, build, check, lsp, help
A <PATH> is a .som file, or a directory of them.
```

> Nice and short. I know to do `som run file.som`. Running bare `som` exits with code 2 but prints the full help, which is friendly.

### 2. Hello world

```js
console.log("hello world")          // 01.som
```
```
error: cannot find variable `console`
   1 │ console.log("hello world")
     │ ━━━━━━━ not found in this scope
```
> Fair. Notably, top-level statements are allowed (no `main` required), which is good for me.

```js
function main() { console.log("hello world"); }   // 02.som
```
```
error: expected ;
   1 │ function main() {
     │         ━ expected a semicolon after the expression
```
> 🤔 The real problem is that `function` isn't a keyword, but it tells me about semicolons. I've got no idea what to do.

```js
print("hello world")     // 03.som
```
```
error: cannot find function `print`
     ┆ `print` is defined in module `som::io` but not is accesable from here. try `som::io::print` or `use som::io;`
     ▪ add `use som::io;`
   1 + use som::io;
```
> 🎉 This is the best moment of the session. It found the function I meant in the std and gave me a patch.
> Nits: there's a typo ("**not is accesable**" should be "is not accessible"). Also `log("…")` suggested `c::log`, which is the C *math* log. A JS person will think it's logging.

```js
use som::io;
println("hello world")    // 06.som  → hello world ✅
```
> But `use som::io; io::println(...)` fails ("cannot find function `io::println`"). So `use` imports everything as a glob, not the module name. That's surprising if you're used to `import * as io`.

### 3. Variables & strings

```js
const age = 30; var x = 5;    // 09.som
```
```
error: expected ;
   3 │ const age = 30;
     │      ━ expected a semicolon after the expression
```
> Again "expected ;". It would be great to get "`const`/`var` aren't keywords, use `let`".

```js
println("hi " + name);   // 10.som
```
```
error: cannot apply `+` to a string and a string
     │         ━━━━━━━━━━━━ no such operation
```
> OK, so how *do* I join strings? There's no hint. I tried:
> - `` `hi ${name}` `` → "expected expression"
> - `println("hi", name)` → "`println` takes 1 argument, but 2 were given"
> - `"hi {name}"` → compiled, printed `hi {name}` literally
> - `format("hi {}", name)` → not found
>
> I gave up and used `print("hi "); println(name);`.

```js
println(age);   // 15.som
```
```
error: expected `cstr`, found an integer
   6 │ println(age);
     │         ━━━ this is an integer
   3 │ pub fn println(text: cstr) {
     │                      ──── `cstr` expected because of this
   4 │     c::printf("%s\n", text);
                                   /Users/lucas/som/std/io/io.som:3:22
```
> I love that it shows the std definition. I learned `fn`, `name: type`, and that `c::printf` exists. But what's a `cstr`? Earlier the error called `"…"` "a string".
> `age.toString()` → "an integer has no method `toString`". `String(age)` → not found.
> `c::printf("age is %d\n", age)` → works ✅ (I've vaguely heard of printf).

#### Discovering type names
```
let x: number = 1;  → cannot find type `number`
let x: string = ""; → cannot find type `string`
let x: int / float  → cannot find type
```
> No suggestions at all for types. Mismatch errors only say "found **an integer**" / "**a float**" / "**a string**" / "**a collection of 2 elements**", never the type I'd actually write. I had to guess Rust names: `i32 i64 f32 f64 u8 usize bool char str cstr` all exist. `String` gave the helpful "defined in `som::data`" hint.
> Odd: `let j: char = 1;` type-checks.

#### Trying to print a String
```js
use som::data;
let a: String = "hi "; let b: String = "bob";
let c = a + b;     // ✅ String + String works
println(c);        // ❌ expected `cstr`, found `String`
```
> So I need String → cstr. I guessed `toString to_cstr as_cstr to_str as_str c_str bytes len length`. Every one gave "`String` has no method `X`", with **no list of what methods exist**.
>
> Then I tried `a.cstr()`:

```
$ som check m_cstr.som      # println(a.cstr());
(no output … killed after 2+ minutes)
```
> 🐛 **The compiler hangs.** Minimal repro:
> ```
> let a = 1;
> f(a.i32);        // also: f(a.cstr()), let b = [a.i32];
> ```
> Outside a call/array it errors correctly ("expected identifier, found `cstr`"). Inside `(...)` or `[...]` the parser seems to loop on recovery.

```js
let x: bool = a.length;   // 31.som
```
```
error: `String` has no field `length`
     ┆ available fields: `bytes`; properties: `len`
```
> Oh nice, **field** errors list what's available. Method errors should too. Also, `a.len()` said "no method `len`" without mentioning that `len` exists as a property and I should drop the `()`.
>
> `a.bytes` → "`bytes` is private to `String`", and then the suggested patch is **"make it `pub`" in `std/data/string.som`**. A user shouldn't be told to edit the stdlib.
> `List`'s "available fields" lists `mem`, `len`, and `len` shows up as both a field and a property. I think it lists private fields as available.
>
> **I never found a way to print a `String`.** I moved on.

### 4. Control flow

```js
for (let i = 1; i <= 15; i++) {   // 40.som
```
```
error: expected identifier, found `(`
     │    ━ expected a name for each value
```
> "a name for each value" made me guess `for … of`:
```
error: missing `in`
   2 │ for i of [1, 2, 3] {
     │      ━ expected `in` and what to loop over
```
> 👍 Very clear. `for i in 1..16 { … }` works, and so does `if (cond)` with parens. FizzBuzz ran on the first try after that.
> `===` gave only "expected expression". It could say "use `==`".
> `switch` → three "expected ;" errors. I guessed Rust's `match x { 1 => …, _ => … }` and it worked.

### 5. Functions

```js
fn add(a, b) { return a + b; }    // 50.som → 5 ✅
```
> Untyped params are 🤯 great for a JS dev. Implicit return of the last expression works too.

```js
add(2, 3); add(2.5, 3.5);         // 52.som
```
```
error: expected an integer, found a float
   5 │ c::printf("%f\n", add(2.5, 3.5));
     │                       ━━━ this is a float
```
> Why is an integer expected? It doesn't point at `fn add` or the first call that fixed the type. I'm confused.

Arrow functions / closures:
```
let add = |a, b| a + b;          → expected expression
let add2 = fn(a, b) { … };       → expected expression + expected a function name
const add = (a, b) => a + b;     → expected ;
```
> No closures, it seems. Also the "expected expression" caret sits on the **space before** the token (column off by one).

### 6. Objects & classes

```js
let person = { name: "Bob", age: 30 };   // 60.som → expected `;` or `}`
class Person { constructor(...) {...} }  // 61.som → expected ;
```
> Nothing points me to `struct`. I only knew it from the `pub struct String` snippet an earlier error showed me.

```rust
struct Person { name: cstr, age: i32 }
impl Person { fn greet(this) { … this.name … } }
```
```
error: cannot infer the type of this expression
  11 │     println(this.name);
     │             ━━━━ add a type annotation
```
> Not helpful: it silently treats `this` as a normal untyped parameter. A hint like "the receiver is called `self`" would save a lot of time. With `self`, it prints "hi, I am Bob" ✅.

### 7. Mutation

```js
count += 1;  count++;     // 70.som → "expected expression" ×2
```
> No compound assignment, and the error doesn't say so.

```js
let count = 0; count = count + 1;
```
```
error: `count` is not mutable
   1 │ let count = 0;
     │     ───── declared without `mut`
   2 │ count = count + 1;
     │ ━━━━━ assigned here
     ▪ make `count` mutable
   1 + let mut count = 0;
```
> 💯 Perfect error.

### 8. Arrays / Lists

```js
let nums = [1, 2, 3]; nums.push(4); nums.length;
```
```
error: `[i32; 3]` has no method `push`
error: `[i32; 3]` is not a struct
     │                   ━━━━ only structs have fields and methods
```
> These two messages contradict each other ("has no method `push`" vs "only structs have methods"). The second also underlines `nums`, not `length`. `[i32; 3]` is alien notation to me.

- `List::new()` → "did you mean `List::get`?" ❌ That's a bad suggestion for a constructor.
- `List::with_capacity()` → takes 1 argument. `List::empty()` → works (once annotated).
- `List::from()` listed both overload candidates with signatures. 👍 Very informative.
- `List::from([1,2,3])` 🐛:
```
error: ambiguous call to `List::from`
     │ candidate `from<T, N>(items: [T; N]) -> List<T>`
     │ candidate `from<T>(items: &[T]) -> List<T>`
     ┆ specify a return type to indicate which overload to use
```
> I added `let mut nums: List<i32> = …` and got the **identical error**. The advice can't work. Also, before the root cause it printed three cascading "cannot infer the type" errors, which made it look like the problem was elsewhere.

Indexing:
```
error: cannot index `List<i32>` with `[]`
     │ only an array's length is known, so only arrays are indexed
     ┆ use `nums.get(0)`, which gives an optional
```
> The hint is good. The reasoning ("only an array's length is known…") confused me.

Optionals:
```
error: `nums.get(0)` might be `none`
     │ this is `&i32?`: check it holds a value before using it
     ┆ check it first: put it in a variable and use it inside `if value { … }`
     ┆ or give a fallback for `none`: `nums.get(0) else …`
```
> 🏆 Best error in the language. It teaches the feature.

`… else 0` → "expected `&i32`… borrow it: `&0`". The prose says `&0` but the patch says `&(0)`, which is inconsistent.
Then 🐛 **silent garbage**:
```js
if v { c::printf("got %d\n", v); }     // prints: got 52614304
c::printf("%d\n", nums.get(99));       // prints: 0   (an optional passed to printf!)
c::printf("%d\n", "hello");            // prints: 48514292
```
> No error, no warning. As a JS dev I would never guess I need `*v`. I only tried it because of C memories.

`for n in nums`, `while`, and `match` all work.

### 9. Errors & crashes

- `throw "…"` → "expected `;` or `}`". There's no `panic` or `assert` either (no "defined in module" hint). Only `c::exit` and `c::abort` exist.
- `10 / 0` (through a variable) → printed `-85999232`, **exit 0**.
- `i32` overflow → wrapped silently.

### 10. Exit codes 😱

```
$ som run 103.som     # c::printf("hi\n")      ← no semicolon
hi
[exit 3]
$ som run 104.som     # let x = 5; x
[exit 5]
$ som run 102.som     # match whose arms return printf(...)
other
[exit 6]
```
> The last top-level expression becomes the process exit code. A missing `;` makes my script "fail" in CI. Very surprising.

```
$ som run 105.som     # "hello"
error: the C compiler rejected the generated code
     ┆ this is a bug in the som compiler
     ! …program.c:17:17: error: operand of type 'struct som_str' where arithmetic or pointer type is required
     !    17 |     return (int)som_main();
```
> 🐛 An internal compiler error (at least it honestly says so and keeps the `.c` file).

### 11. `main`, args, modules, build

- `fn main() { … }` works.
- Top-level code **plus** `fn main` gives:
  ```
  error: `main` is already defined with the same signature
     1 │ c::printf("top\n");
       │ ━ and is defined again here
  ```
  > Baffling. It points at the `c` of `c::printf`. The message should say "top-level statements form an implicit `main`".
- `fn main(args)` → "cannot infer" (printed **twice**, identically). I never found out how to read CLI args, even though `som run` documents `-- ARGS`.
- `import { square } from "./math"` / `export function` → "expected ;". With `pub fn square` in a sibling file, `square(4)` just works without any import. `use math;` → "module `math` not found, looked in `./math`".
- Missing file → "create `app.som`, or a directory `app/` with a `main.som` in it" 👍
- `som build 06.som` → writes `./06`, and `built: …` goes to stderr (so piping stdout is clean) 👍

### 12. A small "real" program (todo list)

Following the errors in order:
1. `first.done = true` → not mutable → add `mut` ✅ (JS lets you mutate a `const` object's fields, so this surprised me, but the fix was given).
2. `todos.push(first); … show(first)` →
   ```
   error: `first` is used after it was consumed
     ┆ `first` is a `Todo`, which moves rather than copies: once it's consumed, it's gone
     ┆ consume it only after its other uses, or only in the branch that needs it
     ┆ or, if a reference is enough where it's consumed, borrow it there: `&first`
   ```
   > Plain-English ownership explanation. Excellent.
   > 🐛 But line 18, `first.done = true` *after* the move, wasn't flagged. Minimal repro: `let b = a; a.x = 5;` passes `som check`.
3. `for t in todos` then `todos.len` → consumed → `for t in &todos` ✅
4. `show(&t)` → "expected `&Todo`, found `&&Todo`… dereference it: `*(&t)`". It's correct but silly; the natural fix is `show(t)`.

Final output:
```
[x] learn som
[x] learn som
[ ] write js
2 todos
```

### 13. Misc
- Typos: `pritnln(name)` with `nmae` declared → "did you mean `println`?" and "did you mean `nmae`?" 👍
- No warnings for unused variables or functions.

---

## Suggested fixes, by priority

| # | Issue | Repro file |
|---|-------|------------|
| 1 | Parser infinite loop on keyword after `.` inside `()`/`[]` | `h6.som`: `let a = 1; f(a.i32);` |
| 2 | Invalid C when the trailing top-level expr is a string | `105.som` |
| 3 | `List::from([..])` ambiguous, and the hint can't resolve it | `84.som` |
| 4 | Field assignment on a moved value accepted | `134.som` |
| 5 | `c::printf` accepts refs, optionals, and `str` for `%d` without complaint | `86.som`, `89.som` |
| 6 | Implicit exit code from the last top-level expression | `103.som` |
| 7 | Div-by-zero and overflow are silent UB | `111.som`, `112.som` |
| 8 | "No method" errors should list methods, and point to a property if one has the name (`len()` → `len`) | `27.som`, `m_len.som` |
| 9 | Don't suggest making std fields `pub` | `30.som` |
| 10 | Keyword-aware parse errors: `const/var/function/class/switch/throw/import/===/+=/++/=>` | `09`, `61`, `70`, `101`… |
| 11 | Type-not-found should suggest names (`int`→`i32`, `string`→`str`/`String`, `number`) | `19–21.som` |
| 12 | A way to print `String`/numbers easily (interpolation or `println` overloads) | `29.som` |
| 13 | Duplicate identical diagnostics; cascade errors printed before the root cause | `120.som`, `83.som` |
| 14 | Implicit-main vs `fn main` conflict message | `122.som` |
| 15 | Small things: "not is accesable" typo; `&0` vs `&(0)`; `*(&t)` vs `t`; caret off by one on "expected expression"; `List::new` → `List::get`; `log` → `c::log` | — |
