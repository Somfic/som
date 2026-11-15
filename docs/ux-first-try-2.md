# som, first try: a newbie's UX log

I played a not-very-bright programmer who has seen some JS, Python, C and a little
Rust. I did **not** read the readme, `docs/`, or the std source. I only had
`som --help` and the compiler's error messages. Everything ran in a scratch dir
against `target/debug/som` (0.1.0, branch `2027` @ 3bbc7c4).

TL;DR: the "you're writing language X, som spells it Y" errors are **excellent**.
They taught me most of the syntax with no docs. The weak spots are (1) dead ends
where no error gives a hint: function values, parsing numbers, tuples, errors and
exceptions, `+=`; (2) generic parser fallbacks (`expected ;`) for whole families of
mistakes; (3) a few auto-fix suggestions that are wrong or render badly; (4) some
silent behaviours that surprised me more than any error did.

---

## 1. CLI

```
$ som
The som compiler
Usage: som <COMMAND>
Commands: run, build, check, lsp, help
...
A <PATH> is a .som file, or a directory of them.
```

- 👍 Short and clear. The line "A `<PATH>` is a .som file, or a directory of them"
  is helpful.
- 👎 Running `som` with no args exits with code 2 and prints the full help. That's
  fine, but there's no `som new` / `som init`, and no `som doc` or any other way to
  see what's in `som::io`. Discovery happens only through errors (see §9).

```
$ som run hello.som          # file missing
error: `hello.som` does not exist
     ┆ create `hello.som`, or a directory `hello/` with a `main.som` in it
```
👍 Nice. (Later I found a directory works **without** a `main.som`, so the hint
oversells the convention.)

- `som run x.som` prints `built: \`x.som\`` (to stderr) before the program output.
  "built" next to the *source* file name reads oddly. `som build` prints
  `built: \`hello\`` (the binary), which makes sense. For `run` I'd drop it or say
  `compiled`.
- `som check` prints `checked: \`s.som\``. Fine.
- `som build hello.som` drops `./hello` next to the source. Fine.
- `som run ar.som -- a b`, where the program prints `args()`:
  ```
  /var/folders/1t/.../T/som-run-35269/program
  a
  b
  ```
  argv[0] is a random temp path. Expected, but it's a small surprise when
  `args()[0]` isn't my program's name.
- `som run` passes the exit code through (`exit(3)` → `exit=3`). 👍

## 2. Hello world

```som
print("hello world")
```
```
error: cannot find function `print`
     ┆ `print` is defined in module `som::io` but is not accessible from here.
       try `som::io::print` or `use som::io;`
     ▪ change to `som::io::print`
     ▪ add `use som::io;`
```
👍 Great: two working fixes, both shown as diffs. Both compiled.

- `print` doesn't add a newline, and my shell prompt ended up on the same line.
  `println` exists (I guessed it).
- 🤔 Mental model clash: `use som::io;` imports the *contents* (`print` works bare),
  but `io::print(...)` then fails with `cannot find function io::print`, with no
  hint. Coming from Rust or Python I expected `use som::io` to bind `io`. The error
  should say "`use som::io;` brings in `print` itself; write `print`".
- Top-level statements just work, so no `main` is needed. 👍 (But see §8: two
  files with top-level code → an error about `main`, which I never wrote.)
- A missing `;` on the **last** line is accepted (`print("hi")`), but a missing `;`
  mid-file is an error. Inconsistent, though forgivable.

## 3. Variables

| I wrote | Compiler said | Verdict |
|---|---|---|
| `x = 5` (no `;`) | `error: expected ;` ← no backticks around `;` | 👎 doesn't mention that `x` isn't declared |
| `x = 5;` | `cannot find variable x` ×2 (assignment + use) | 👎 should suggest `let x = 5;` |
| `let x = 5` | ``expected `;`, found identifier`` + fix | 👍 |
| `var x = 5;` | "som declares variables with `let mut`, not `var`" + fix | 👍👍 |
| `const X = 5;` | "som declares variables with `let`, not `const`" | 👍 |
| `let x = 5; x = x + 1;` | "`x` is not mutable … make `x` mutable" + diff | 👍 |

Nit: the two "expected semicolon" errors are formatted differently: `expected ;`
vs ``expected `;`, found identifier``.

## 4. Strings and printing

- `"x is " + x` (string + int) **just works**, and so do `1 + "a"` and
  `"a" + 1.5`. Surprising, but convenient.
- Interpolation: `"x is {x}"` works, `"{x + 1}"` works, and `{{` / `}}` escape.
  The errors for a lone `{` or `}` are great ("for the brace itself, write `{{`").
- `f"…"` → "som's text has no `f` in front", plus a fix. 👍
- `"{y}"` with an undefined `y` → "did you mean `x`?" with a fix. 👍
- ⚠️ **`"x is ${x}"` silently prints `x is $5`.** Every JS/TS/Kotlin/shell
  programmer will write this. It deserves a warning or error: "`$` before `{…}`
  is printed literally; did you mean `{x}`?"
- `"\{x}"` → `unknown escape \`\`` (the character after `\` is missing from the
  message), and the underline covers the whole string instead of the escape.
- `'c'` → bare `expected expression`. Should say "som has no character
  literals" or "use double quotes". (Strangely, `\'` *is* in the escape list.)
- `` `x is ${x}` `` → bare `expected expression`.
- `"a" * 3`:
  ```
  error: cannot apply `*` to `string` and an integer
     ┆ to give it `*`, implement `Multiply` for the type
     ▪ implement `Multiply` for `string`
   6 - }
   6 + }
  impl Multiply for string {
      fn multiply(self: &string, other: &string) -> string { }
  }
  ```
  👎👎 Three problems: (1) the file has 3 lines but the diff talks about line 6;
  (2) the generated signature takes `other: &string`, which can't help with
  `string * int`; (3) telling a newbie to implement an operator on the built-in
  `string` is the wrong nudge. "Strings can't be repeated with `*`" would do.

### Printing non-strings
- `println(5)` works and `println(x)` with an int works, but `println(xs)` with an
  array, `println(p)` with a struct, and `println(args())`:
  ```
  error: expected `string`, found a collection of 3 elements
     ... pub fn println(text: string)   (std/io/io.som:3:22)
  ```
  So ints quietly become strings, but nothing else does, even after I implemented
  `Display` on my struct (`"{p}"` works, `println(p)` still errors). That's
  confusing: either everything `Display` converts or nothing does.
- The message calls the array "a collection of 3 elements" here and `[i32; 3]`
  two errors later. Pick one.
- No hint like "write it into text with `"{p}"`".

### Implementing Display (painful)
`"{p}"` gave the best hint: "`Point` can't be written into text … implement
`Display` for it: `prop display_len` says how many bytes its text takes, and
`display` writes them". Then:

1. An empty `impl Display for Point {}` → a good "missing `fn display_len(...)` and
   1 more" error with a stub. 👍 But the hint said **`prop`** display_len and the
   stub says **`fn`** display_len. Which one?
2. `display(self: &Point, to: *u8)`: a **raw pointer**, for printing a point.
   As a newbie I had no idea what to do with `to`. I tried `to = s;` and got
   "`to` is not mutable → make it `mut to`", which **leads the wrong way**
   (reassigning the pointer can't write text), plus a type error.
3. I only got it working by provoking `s.foo()`, seeing that `string` has a
   `display` method, and delegating: `"({self.x}, {self.y})".display(to)`. That
   formats the string twice. A Display that can't be written without trickery is
   a big wall for beginners.

## 5. Functions

- `function add(a, b)` → "som declares functions with `fn`, not `function`". 👍
- Untyped params `fn add(a, b)` **work**. Calling `add(1,2)` and then
  `add("a","b")` gives:
  ```
  error: expected `i32`, found a string
     6 │ println(add("a", "b"));
  ```
  👎 I never wrote `i32`. The error needs to say *why* it's `i32` (inferred from
  `add(1, 2)` on line 5, or "untyped params take the type of the first call").
  When the param **is** annotated, the same error nicely shows
  "`i32` expected because of this".
- `fn add(a: int, b: int): int {` → **`missing {`**. 👎 No hint that the
  return type uses `->`. This is the TypeScript/Kotlin/Python-annotations way of
  writing it; a "som writes `-> T`" error would fit the other X-vs-Y messages.
- `int` → "som writes `i32`: `int` is how C, C++, Java… write it". 👍 But you get
  three identical errors for three `int`s, each with its own one-token diff.
  A single "replace all 3" would be nicer.
- `float` → no auto-fix (it lists `f32`/`f64`, fair enough).
- Arity errors (`takes 2 arguments, but 1 was given`) are clean. 👍
- `a + b;` as the last line of a function returning `i32`:
  ```
  error: this value is unused     ← "working it out does nothing else"
     ┆ use it, or if dropping it is the point, `drop(…)` it
  error: not every path returns `i32`
  ```
  👎 This is the #1 Rust-newbie mistake and the fix is "remove the `;`". Neither
  error says so. "working it out does nothing else" is odd phrasing, and telling
  someone to `drop()` an integer is strange.
- `if b != 0 { a / b }` as a function body → the same pair of errors. Should say
  "an `if` without `else` has no value".
- `fn inc(x) = x + 1;` (expression bodies) is nice. I only learned it from the
  lambda error message.

## 6. Control flow

- `for (let i = 0; i < 3; i++)` → "som loops over a range: `for i in 0..n`…"
  with a correct rewrite to `for i in 0..3`. 👍👍 (The label oddly blames
  "`++` is how C… write it" while underlining the whole header.)
- `n++` → "som has no `++`: write `n = n + 1`". 👍
- ⚠️ `n += 1` → bare **`expected expression`** pointing at `=`. Whoever writes
  `++` will try `+=` next. Either support it or give it an X-vs-Y message.
  (I assume it doesn't exist since `++` suggests `n = n + 1`.)
- `if (x > 3)` with parens is accepted silently. Fine.
- `&&` → "som writes `and`". 👍 But `!false` is accepted, so `&&`/`||` are
  banned while `!` is allowed. A bit inconsistent.
- `if x {}` with an int `x` → "expected `bool`, found an integer". OK (no
  "write `x != 0`" hint).
- `switch` → "som has no `switch`: it's `match x { 1 => …, _ => … }`". 👍
- `try {} catch`, `throw "bad"`, `interface Foo {}`, `import io;` → all give the
  generic **`expected ;`**. 👎 These are exactly the keywords the X-vs-Y system
  should catch. I never found out how som handles errors.

## 7. Collections

- `[1,2,3]` works, and `xs.length` → "som calls it `len`" with a fix. 👍
- `xs.push(4)` on an array → "has no method push; it has methods: get, get_ref,
  …". 👍 The method list is an excellent discovery tool. But nothing says
  "arrays are fixed-size; use `List` for a growable list".
- `xs[10]` on a 3-element array → compile-time out-of-bounds. 👍
- Finding the growable type:
  - `Vec::new()` → `cannot find function Vec::new`, **no hint**.
  - `let v: Vec<i32>` → "som writes `List`: `Vec` is how other languages write
    it". So the hint exists in type position but not in expression position.
    ("other languages" — it's Rust, you can say so.)
  - `List::new()` → "`List` has no `new`; to make one, use `List::empty`,
    `List::from`, `List::of` or `List::with_capacity`". 👍👍
  - `vec![1,2]` → ``expected `;`, found `!` ``. No macro hint.
  - `let v = [];` → "cannot infer the type", but it doesn't show *how* to annotate.
- `let xs = List::empty(); xs.push(1); xs.push(2);` → **two** identical
  "`xs` is not mutable" errors with the same fix. Collapse them. "borrowed as
  mutable here" is jargon for a newbie.
- `println(xs[0])` right after two pushes:
  ```
  error: index `0` is possibly out of bounds
     ┆ nothing is known about `xs.len` here
     ▪ check the length first: `if xs.len > 0 { … }`
     ▪ use `xs.get(0)`, which gives an optional
  ```
  The prover is neat, but I *just pushed two items*, so it "should know". For a
  newbie this feels like the compiler being dumb. Taking the `.get(0)`
  suggestion then revealed a **different** error: `for x in xs` had consumed
  `xs`. The move error itself is very good ("consumed here, gone after this
  point … borrow it there: `&xs`"), and `for x in &xs` fixed it.
- Optionals: `y + 1` where `y: &i32?` → "`y` might be `none` here … check it
  first, as in `if y { … }`, or give a fallback, as in `(y else …)`". 👍 Very
  readable. But `(xs.get(5) else 0)` → "expected `&i32`… borrow it: `&0`".
  Writing `&0` is a weird thing to ask of a beginner; references leak everywhere.
- `Map::empty()`, `insert`, `get` work, but `m.get("a")` needs **`&"a"`**. Same
  complaint. The Map method list has no `remove`/`contains`/iteration (maybe
  that's on purpose).
- ⚠️ `println(xs.get(10))` prints `none`, and `"hi {name}"` with a `string?`
  prints `hi none` when stdin is empty. Optionals interpolate silently, which
  goes against the strict `none` checking everywhere else.

## 8. Structs, methods, enums

- `class Point { x: i32; y: i32; }` gives three errors:
  1. "som declares structs with `struct`, not `class`" 👍
  2. "items are separated with `,`, not `;`" 👍
  3. "missing `}`" with fix **`y: i32};`** 👎 wrong (cascade from #2; it should
     have been the same `,` fix).
- `new Point(1, 2)` → "som has no `new`: make a struct with `Point { … }`". 👍
- `Point { 1, 2 }` (positional) → **three** cascading parse errors
  (`expected ;, found {`, `expected ; or }`, `expected expression`). 👎 Should
  be one error: "struct fields need names: `Point { x: 1, y: 2 }`".
- Missing field `y` → clear. 👍
- Extra field `z: 3` → "did you mean `x`?" and the fix produces
  `Point { x: 1, y: 2, x: 3 }`, a **duplicate field**. 👎 It should suggest
  removing `z`.
- `p.to_string()` → "has no method to_string; it has fields: x, y". A
  "use `"{p}"`" hint would help here.
- Methods: `fn inc(self) { self.n = … }` → "`mut self` would change the method's
  own copy… take it by mutable reference: `&mut self`". 👍👍 Excellent.
- `fn get(self) -> i32` and calling `c.get()` twice → "`c` is used after it
  was consumed … borrow it there: `&c`". 👎 The real fix is `fn get(&self)` in
  the *method signature*; the error never points at the signature.
- Two `self` styles show up: stubs say `self: &Point`, the hint says `&mut self`.
- `enum Color {…}; Color.Red` → "cannot find variable `Color`". 👎 No
  "use `Color::Red`".
- `match` exhaustiveness: "`Green` and `Blue` aren't matched" with an add-arms
  fix. 👍 But the **multi-line diff renders broken**: lines lose their gutter
  and `}` lands in the wrong place:
  ```
     5 -   Red => println("r"),
  }
     5 +   Red => println("r"),
      Green => {},
      Blue => {},
  }
  ```
  The same happens in the `if xs.len > 0 { … }` suggestion. And if I wrote
  `Color::Red =>`, the suggested arms are still bare `Green`/`Blue`.
- Enum payloads + `match` + `Shape::Circle(2)` → "expected `f64`, found an
  integer" pointing at the variant's field type. 👍 But **no hint to write
  `2.0`**. Strict float literals are fine, but say so.
- `let f: f64 = n` (i32) → "expected f64, found i32". No `as f64` hint,
  although the `i8` overflow error does suggest `as`.

## 9. Things I couldn't do at all

These are the real UX blockers: I tried several spellings and no error pointed
anywhere.

1. **Call a function passed as a parameter.** `fn twice(f: fn(i32) -> i32, x:
   i32) -> i32 = f(f(x));` → "cannot find function `f`" (it's a parameter right
   there!). `(f)(x)` gives the same. `f.call(x)` → "`fn(i32) -> i32` has no method
   `call`". The lambda error *told me* to "declare one with a name … and pass
   that", so passing works but calling doesn't. Also the wrong guesses got broken
   auto-fixes: `Fn(i32) -> i32` → suggests `Fn,(i32) -> i32`; `(i32) -> i32` →
   suggests `(i32)) -> i32`.
2. **Parse a string into an int.** `line.parse()`, `parse_int`, `i32::parse`,
   `i32::from`, `int(line)`, `line as i32` all fail. `as` says "only numbers
   convert with `as`", which is honest but no help. Maybe it doesn't exist yet;
   if so, the error could say that.
3. **Tuples.** `let t = (1, "a")` → "missing `)`", then `t.0` → "missing `,`"
   with fix `println(t,.0)`. 👎 Nonsense fix.
4. **Error handling.** `try/catch/throw` → `expected ;`. No idea how failures
   are meant to be reported.
5. **Lambdas.** `(x) => …` and `lambda` get a good "som has no anonymous
   functions" message 👍, but `|x| x + 1` and `fn(x) {…}` (the two most likely
   spellings for a Rust user) get bare `expected expression`.
6. **Seeing what a module has.** `use som::nope;` nicely lists `som::io`,
   `som::mem`, and says `List`, `Map`, `string`, `Rc` need no `use`. 👍 But
   there's no way to list *functions* in `som::io`. I found `read_line` only
   because `readline` got a did-you-mean.
   - `use std::io;` → "module not found", no "did you mean `som::io`?"
   - `#include <stdio.h>` → "som writes a comment with `//`" and fixes it to
     `//include <stdio.h>`. Funny, but wrong.

## 10. Silent surprises (no error at all)

These bothered me more than any bad error:

| Code | Result |
|---|---|
| `let s: string = 5;` / `= true;` / `= add(1,2);` | compiles. Anything converts to `string` silently, but `let n: i32 = "hi"` is an error. |
| `"x is ${x}"` | prints `x is $5` |
| `println(xs.get(10))`, `"hi {name}"` with `name: string?` | prints `none` / `hi none` |
| `let big: i32 = 2147483647; println(big + 1);` | `-2147483648` |
| `fact(20)` in i32 | `-2102132736` |

The overflow one stands out. The compiler proves at compile time that
`n / b` might divide by zero ("nothing is checked as the program runs") and that
`xs[0]` might be out of bounds, yet a **constant** `i32::MAX + 1` wraps with no
warning. With the prover that strict, I assumed overflow would be caught too.

## 11. What's genuinely great

- The **"X is how Java/C#/… write it"** messages: `var`, `function`, `class`,
  `new`, `switch`, `++`, C-style `for`, `&&`, `null`, `String`, `int`, `length`,
  `f"…"`, `#` comments, `lambda`, `=>`. They taught me most of the syntax. This
  is the best part of the experience.
- **Method/field lists** on "has no method": the best discovery tool available.
- **Borrow and move errors** are written in plain words ("consumed here, gone
  after this point"), not "E0382 use of moved value".
- `&mut self` explanation, missing-match-arm fixes, missing-impl-method stubs,
  arity errors, privacy across files ("`double` is private to `util.som` …
  make it `mod`"), and infinite-recursion detection.
- The layout (code frame, `━` for the main span, `─` for secondary spans, path
  on the right) is clean and readable.

## 12. Top fixes, ranked by how much they'd help a newbie

1. Make function-typed params callable, or explain why not (§9.1).
2. Turn the generic `expected ;` / `expected expression` fallbacks into X-vs-Y
   messages for `+=`, `try/catch/throw`, `interface`, `import`, `|x|`,
   `fn(x){}`, `'c'`, `: T` return types, tuples, positional struct literals
   (§3, §5, §6, §9).
3. Fix the broken suggestion rendering: multi-line diffs, phantom line numbers in
   the `impl Multiply` fix, `Fn,(`, `(i32))`, `t,.0`, `x: 3` duplicate field,
   `y: i32};` (§4, §8, §9).
4. Warn on `${x}` in strings, and on interpolating an optional (§4, §7).
5. Explain *where* an inferred type came from for untyped params (§5).
6. `;` after the tail expression → suggest removing it (§5).
7. Make `println` accept anything `Display`, or explain why ints are special
   (§4).
8. Give `Display` a friendlier writer than `*u8`, and settle on `prop` vs `fn`
   for `display_len` (§4).
9. Merge duplicate errors (same variable not mutable twice, `int` ×3) (§5, §7).
10. Hints for `Color.Red` → `Color::Red`, `Vec::new` → `List::empty`,
    `std::io` → `som::io`, `2` → `2.0`, `n` → `n as f64` (§7, §8).
11. Silent `T → string` conversion in typed `let`s and silent overflow:
    decide whether they're on purpose (§10).
12. Multiple files with top-level code → "`main` is already defined with the
    same signature". Say "only one file may have top-level statements" (§8).
