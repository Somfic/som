# First time trying som (as a not-very-smart programmer)

Rules I followed: no readme, no peeking at `std/`, no docs. All I had was `som --help` and the error messages.
Binary: `target/debug/som` (built with `cargo build -p som-cli`). All programs were written in a scratch folder.

---

## TL;DR: what I'd tell the author

**Things that were really nice**
- The "you wrote it like language X" errors are excellent. `function`, `def`, `var`, `class`, `elif`, `switch`, `#` comments, `null`, `True`, `&&`/`||`, `not`, `++`, C-style `for`, `int`, `.length` all gave me the som spelling, the language I was thinking of, and a diff I could paste. I learned most of the syntax this way.
- The empty-file message ("welcome to som! … happy hacking :)") is a lovely first contact.
- Missing-import errors tell me which module has it (`println` is in `som::io`) and give me both fixes.
- Unknown-method errors list the methods that do exist. Unknown-module errors list the modules there are.
- The move / mutability / "not every path returns" errors explain why, not just what.
- Top-level statements with no `main` made hello world a one-liner.

**Bugs / footguns (the compiler let me write wrong programs)**
1. **Segfault:** `println("got " + read_line());` (string + `string?`) compiles and segfaults (exit 139) when stdin has a line. With empty stdin it prints `got none`. It should be a type error, like `let x: i32 = read_line()` already is.
2. **`som run` hides crashes:** that segfault shows up under `som run` as a plain `exit=1` with no message. I only found out it was a crash by running `som build` and then the binary myself.
3. **Untyped params lock to the first call's type, and strings swallow everything:**
   ```som
   fn add(a, b) { return a + b; }
   println(add("a", "b"));   // ab
   let n = add(1, 2);
   println(n);               // 12   <-- !!!
   let m = n + 1;            // compiles too
   ```
   And when the strings call comes first, `add(1.5, 2)` gives `1.52` and `add(true, 2)` gives `true2`. If the int call comes first, a later float call is a type error instead. Same code, different meaning depending on call order.
4. **Implicit number→string conversion:** `let s: string = 5;` compiles. (`let i: i32 = "5";` does not, which is good.)
5. **Divide by zero:** `10 / a` with `a = 0` prints `0`. No crash, no warning.
6. **Integer overflow wraps silently:** my sum of args gave `-947453315` (because of the `args()[0]` thing below).

**Places I got stuck or confused**
- `/* block */` comments: "expected expression", and the caret points at the end of **line 1**, not the comment.
- `'hi'` and `'0'` (single quotes): just "expected expression". No "use double quotes" or "no char literals" hint.
- `+=`: "expected expression". Every other C-ism has a nice hint but this one doesn't.
- `fn f(a: int): int` (TS/Kotlin return syntax): "missing `{`", with no hint that it's `-> T`.
- **Parsing a string to a number:** I couldn't find a way at all. I tried `.parse()`, `.to_int()`, `i32::parse`, `i32::from`, `parse_int`, `int(…)`, `as i32`. None of them got a hint. `string::xyz` doesn't list string's static functions (but `List::new` does list List's, so that pattern already exists). In the end I wrote a parser by hand with `.bytes`.
- `Vec::new()`: "cannot find function". Nothing points me to `List`. (`List::new` gives a great hint, but I had to already guess `List`.)
- `args()` includes the program path as element 0, and under `som run` that's a temp path (`/var/folders/.../som-run-25047/program`). Nothing told me. I only found out because my sum was garbage.
- `List` indexing (`a[i]`) gives an optional (`&string?`), but array indexing must be provably in bounds. Two different rules for `[]`. Also `println(a[0])` happily printed an optional with no complaint.
- After I fixed the method to need `&mut self`, calling the by-value `bark(self)` "consumed" `d`. The hint said "borrow it there: `&d`". I did that literally (`&d.bark();`), got "this value is unused", then `(&d).bark();`, and only then got "have the method take `self: &…`". The real fix was `fn bark(&self)`, and the hint should say exactly that (with a diff). Also `self: &…` is a different spelling from the `&mut self` it suggested a minute earlier.
- `let mut n = 0; for b in s.bytes { n = n*10 + (b-48) }`: "expected `i32`, found `u8`" at the `return n`. It doesn't say *why* `n` became `u8` (inferred from `b`). Then "cannot apply `+` to `i32` and `u8`" gave no hint to use `as i32`.
- `x = 5;` with no `let`: "cannot find variable `x`". Could suggest `let x = 5;`, since that's what a Python/JS person means.
- Mixed operator style: `and`/`or` are words, but `!` is a symbol and `not` is rejected. I tried `not` *because* `and` was a word.

**Error-message polish**
- Typo: `error: the program has an no entrypoint`.
- The missing-match-arm suggestion renders badly. The `}` drops out of the gutter, indentation is lost, and it suggests `Blue =>` while my other arms say `Color::Blue`:
  ```
     ▪ add an arm for `Blue`
  12 -     Color::Green => println("green"),
  }
  12 +     Color::Green => println("green"),
      Blue => {},
  }
  ```
- `str` → "text used to be `str`". As a newcomer I don't know the language's history, so just say "som calls it `string`".
- Three `int` errors in one signature give three separate suggestions, each fixing one occurrence. One "replace all `int` with `i32`" would be nicer.
- Two identical "`nums` is not mutable" errors for two `push` calls on the same variable.
- Unclosed `{`: the fix puts `}` at end of file (`f();}`). It would help to also point at the `{` that was never closed.
- C-style for: the label says "`++` is how C … write it" under the whole `(let i = 0; i < 5; i++)` header. It's about the loop form, not `++`.
- `som run` prints `built: \`hello.som\`` before the program output. That's noise when I just want to run, and it says `.som` while `build` says the binary name.
- `let i = 7; xs[i]` on a 3-element array says "might be out of bounds". It definitely is.

---

## The log

### Getting started

```
$ som
The som compiler
Usage: som <COMMAND>
Commands: run, build, check, lsp, help
A <PATH> is a .som file, or a directory of them.
```
Clear. I'll do `som run`.

```
$ touch hello.som && som run hello.som
error: the program has an no entrypoint
     ┆ welcome to som!
     ┆ to get started, write your first lines in `hello.som`, like `println("Hello, world!");`
     ▪ print a `"Hello, world!"`
   1 + use som::io;
   2 +
   3 + println("Hello, world!");
```
Aww, cute. (Typo "an no".) I'm lazy, so I skipped the `use` line:

```
$ som run hello.som          # println("Hello, world!");
error: cannot find function `println`
     ┆ `println` is defined in module `som::io` but is not accessible from here. try `som::io::println` or `use som::io;`
```
Fair enough, added it:
```
built: `hello.som`
Hello, world!
```
🎉 Took 3 tries, but all of them were guided.

### Variables
- `x = 5` with no semicolon → "expected ;". OK, semicolons.
- `x = 5;` → "cannot find variable `x`" (twice, once per use). I had to guess `let`.
- `let x = 5; println(x);` → `5`. println takes ints? Cool.
- `var x = 5;` → "som declares variables with `let mut`, not `var`" + diff. 👍
- `let x = 5; x = x + 1;` → "`x` is not mutable … make `x` mutable" + diff. 👍
- `println("x is " + x)` with int `x` → `x is 6`. Nice. (Later I learned this is the same mechanism behind bugs 1, 3 and 4.)

### Functions
- `function add(a, b)` → told to use `fn`. `def` → also told `fn`. 👍
- `fn add(a, b) { return a + b; }` with no types at all → works, `3`. Neat!
- …then `add("hi ", "there")`, `add(1.5, 2)`, `add(true, 2)` → `hi there`, `1.52`, `true2`. Huh? See bug 3.
- `fn add(a: int, b: int): int` → "missing `{`" at the `:`. No hint. I guessed `->`.
- `-> int` → three "cannot find type `int` … som writes `i32`" errors. OK, `i32`.
- `1.5 + 2` → "cannot apply `+` to a float and an integer". Fine, but a beginner would love "write `2.0`".

### Strings
- `let s: str = 5;` → "som's text is `string` … text used to be `str`". Used to be? I just got here.
- `let s: string = 5;` → compiles?! `let i: i32 = "5";` → error. Hmm.

### Loops and ifs
- `for (let i = 0; i < 5; i++)` → "som loops over a range: `for i in 0..n`" + diff. 👍
- `for i in 0..5 { println(i); }` → 0..4. 👍
- `while (i < 10)` with parens → fine.
- `elif` → `else if`. `i++` → `i = i + 1`. 👍
- `i += 1;` → "expected expression". 😕 Wrote `i = i + 1`.

### Lists
- `let nums = [1, 2, 3]; nums.push(4);` → "`[i32; 3]` has no method `push` … it has methods: `get`, `get_ref`, `get_mut`, `set`, `slice`, `slice_mut`; properties: `len`". So fixed-size arrays. Helpful listing.
- `println(nums)` → expected `string`, found `[i32; 3]`, and it showed me `pub fn println(text: string)` from the std. OK, println only takes strings (plus whatever auto-converts?).
- `.length` → "som calls it `len`". 👍
- `Vec::new()` → "cannot find function". No hint. Guessed `List::new()` → "`List` has no `new`; use `List::empty`, `List::from`, `List::of` or `List::with_capacity`". 👍
- `[]` → "cannot infer the type of this expression" (plus no `push`).
- `List::empty()` + `push` → two separate "`nums` is not mutable" errors. Added `mut`.
- `for n in nums {…}` then `nums[0]` → "`nums` is used after it was consumed … borrow it there: `&nums`". Clear. `for n in &nums` works.

### Structs and methods
- `class Dog` → `struct`. 👍  `Dog { name: "Rex", age: 3 }` works.
- `impl Dog { fn bark(self) {...} }` → works first try (Rust muscle memory).
- `fn birthday(self) { self.age = ... }` → "`self` is not mutable … `mut self` would change the method's own copy … take it by mutable reference: `&mut self`". Excellent explanation.
- then `let d` → needed `let mut d`. Fine.
- then `d.bark()` "consumed" `d`, so `println(d.age)` failed. The hint said to borrow with `&d`. Tried `&d.bark();` → "this value is unused". Tried `(&d).bark();` → "the method takes the `Dog` itself … have the method take `self: &…` instead". Changed to `fn bark(&self)` → works. That cost me 3 rounds; see the TL;DR.

### Enums / match
- `switch (c) { case …: }` → "som has no `switch`: it's `match x { 1 => …, _ => … }`". 👍 (no diff, but the shape is given)
- `match` missing `Blue` → "`Blue` isn't matched", with a garbled multi-line suggestion (see polish).
- Bare `Blue =>` (no `Color::`) works. `let s = match n { 1 => "one", _ => "many" };` works.

### Input
- `input()` → not found, no hint. `readline()` → "did you mean `read_line`?" 👍
- `print("name? "); let name = read_line(); println("Hello " + name);`:
  ```
  $ echo Bob | som run in.som
  built: `in.som`
  exit=1                       # no output at all, not even the prompt
  $ som run in.som </dev/null
  What's your name? Hello none
  $ som build in.som && echo Bob | ./in
  exit=139                     # segfault
  ```
  Minimal repro: `use som::io; println("got " + read_line());`.
  The right way (which I found via `let x: i32 = read_line()`, whose error explains `if name { … }` / `name else …`):
  `let name = read_line(); if name { println("got " + name); }` → `got Bob`.

### Command-line args: summing numbers
- `args()` guessed right. `som run args.som -- one two` → `a.len` is `3`. Huh, 3? (Turns out `a[0]` is a temp path.)
- Parsing a number: stuck. Every guess failed (see TL;DR). Wrote it myself:
  - `'0'` → "expected expression" (no char literals?). Used `48`.
  - `n` became `u8` without telling me why. Annotated `let mut n: i32 = 0`.
  - `i32 + u8` → no hint. Guessed `as i32`.
  - Ran it → `-947453315`. Turned out to be because `args()[0]` is the program path.
  - Looped `for i in 1..a.len` with `to_number(&a[i])` → "found `&(&string?)`". Dropped the `&` → hint to use `a[i] else …`. `a[i] else ""` → "borrow it: `&\"\"`". Then:
  ```
  $ som run sum3.som -- 1 2 39
  42
  ```
  Final program:
  ```som
  use som::io;

  fn to_number(s: &string) -> i32 {
      let mut n: i32 = 0;
      for b in s.bytes {
          n = n * 10 + (b - 48) as i32;
      }
      return n;
  }

  let a = args();
  let mut total = 0;
  for i in 1..a.len {
      total = total + to_number(a[i] else &"");
  }
  println(total);
  ```

### Grab-bag of mistakes
| I wrote | som said | verdict |
|---|---|---|
| `# comment` | use `//` + diff | 👍 |
| `/* block */` | "expected expression" at end of line 1 | 👎 wrong location, no hint |
| `null` | use `none` + diff | 👍 |
| `True` | use `true` + diff | 👍 |
| `&&` / `\|\|` | use `and` / `or` + diff | 👍 |
| `not x` | use `!` + diff | 👍, but inconsistent with `and` |
| `'hi'` | "expected expression" | 👎 |
| unclosed `fn f() {` | "missing `}`", suggests `f();}` at EOF | 😐 |
| no return in `-> i32` fn | "not every path returns `i32`" | 👍 |
| `1 + 1` as last expr | works (implicit return) | 👍 |
| `10 / 0` | prints `0` | 👎 silent |
| `fn main() {…}` | works | 👍 |
| `printn("hi")` | did you mean `print`? | 👍 (though I meant `println`) |
| `xs[7]` on 3-elem array | "can't tell `i` is inside the array … might be out of bounds" + `get` hint | 👍 (could say "is") |
| `som check t.som` | `checked: \`t.som\`` | 👍 |
| `som --version` | `som 0.1.0` | 👍 |

## Overall feeling
As a dumb programmer I got surprisingly far on error messages alone. The "that's how language X writes it" errors are the best part, and almost every syntax guess was corrected with a diff I could paste. The rough spots are: (a) the compiler being too *permissive* about strings (auto-conversion, untyped params becoming string concatenation, optionals in `+`), which produced wrong output and one segfault instead of errors; (b) a few parser errors that just say "expected expression" (`+=`, `'x'`, `/* */`); and (c) no way to discover things I don't already know the name of (parsing numbers, `List` vs `Vec`, what `string` can do statically).
