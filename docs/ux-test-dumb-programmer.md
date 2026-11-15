# UX test: a "dumb programmer" tries som for the first time

**Persona:** someone who has written a bit of Python and JavaScript, doesn't read docs, and copies
whatever the compiler tells them. No README and no std source; the only teachers are `som --help` and
the error messages. (Std source did show up inside some error messages, and I read it when it did.)

**Setup:** `cargo build`, then `target/debug/som` from a scratch directory. A tiny helper printed the
file and ran `som run <file>`. Date: 2026-11-15, branch `2027`.

---

## TL;DR

**What's great**

- The "som writes X, not Y" errors are excellent. `var`, `const`, `def`, `function`, `class`, `new`,
  `switch`, `elif`, `||`, `not`, `null`, `#` comments, `range(10)`, C-style `for`, `++`, `int`,
  `string`, `float` and `if x = 5` all got a clear message naming my home language, plus a fix I could
  paste. I learned most of the syntax this way, which is exactly the point.
- Ownership errors (`used after it was consumed`, `not mutable`, `cannot assign through a & reference`)
  are worded for humans: "consumed here, gone after this point".
- Optionals (`xs.get(i)`, `if v { … }`, `v else 0`) were easy to pick up from the errors alone.
- The CLI is small and obvious. `som run missing.som` tells you to create the file, status goes to
  stderr, and the program's exit code passes through.

**What blocked me or bit me (worst first)**

1. 🐛 **`print` passes the user's string to `printf` as the format.** `print("score: 100%d %s %s\n")`
   printed `score: 1001829775952 � (null)`. That's junk memory, and with `%n` it could be worse.
   `println` uses `"%s\n"` and is fine.
2. 🐛 **`fn main(args: &[cstr])` type-checks but produces invalid C** (`som_main()` is called with no
   arguments). I never managed to read a command-line argument.
3. 🐛 **Division by zero prints garbage silently** (`10 / 0` → `-85999232`, exit 0). Integer overflow
   also wraps silently.
4. 🐛 **A som optional passed to `c::printf` compiles and prints the "has a value" flag**:
   `c::printf("%d\n", xs.get(1))` prints `1` and not the element. `v.unwrap()` is rejected with
   "`v` might be `none`", but handing the raw optional to a C variadic is accepted.
5. 😵 **I couldn't print a number with som's own API.** `println(5)` → "expected `cstr`". `str(a)`,
   `a.to_string()`, `format`, `print_int` and `a as cstr` all failed with no hint. I only got there by
   copying `c::printf` out of the std source that one error message showed me.
6. 😵 **`str` vs `cstr`.** The compiler told me "som writes `str`" for `string`. Then
   `println(y: str)` failed with "C can't take a `str`" (I hadn't written any C). Worse, a literal's
   type depends on later use: `let a = "bob"; a.len` works, but add `println(a);` and `a.len` becomes
   "`cstr` is not a struct".
7. 😵 **Strings can't be joined or compared, and there's no hint.** `"a" + b` and `a == "bob"` both just
   say "no such operation". The `cstr` version suggests "implement `Eq` for the type", which a beginner
   can't do for a built-in type.
8. 😵 **No way to discover the std library.** `use som::vec`, `som::list`, `som::collections`,
   `som::env`, `som::os` and `som::sys` all say "module not found" without listing what *does* exist.
   I couldn't find a growable list, user input or program args.
9. 😵 **The empty-directory welcome message gives a broken example.** It suggests
   `println("Hello, world!");`, which fails without `use som::io;`. It also has a typo: "the program has
   **an no** entrypoint " (with a trailing space).

---

## Session log

Below, `$` is the command and the block under it is the (sometimes trimmed) output.

### 1. The CLI

```
$ som
The som compiler
Usage: som <COMMAND>
Commands: run, build, check, lsp, help
A <PATH> is a .som file, or a directory of them.
(exit 2)
```

Clear. `som run --help` explains `-- <ARGS>`, `--emit`, `--cc`. 👍

```
$ som run hello.som          # file doesn't exist yet
error: `hello.som` does not exist
     ┆ create `hello.som`, or a directory `hello/` with a `main.som` in it
```

Nice: it tells me what to do.

### 2. Hello world, Python-style

```som
print("hello world")
```
```
error: cannot find function `print`
     ┆ `print` is defined in module `som::io` but is not accessible from here. try `som::io::print` or `use som::io;`
     ▪ change to `som::io::print`
     ▪ add `use som::io;`
```

**Thought:** Really good. Both fixes work. Top-level code with no `main` works, which is very friendly.
`print` adds no newline, and I guessed `println` correctly.

The welcome screen (see §16) suggests `println(...)` without the `use`, so the first program a newcomer
copies from the compiler fails.

### 3. Variables

```som
use som::io;
name = "bob"
print("hello " + name)
```
```
error: expected ;    (at line 2)
```

Odd that `println("hello world")` on the last line didn't need a `;`, but this one does. After adding
semicolons:

```
error: cannot find variable `name`   (×2, once for the assignment and once for the use)
```

**Thought:** No hint that I need to *declare* it. Every other "how Python does it" case has a nice hint,
and a bare `x = 1` is probably the most common Python habit of all. I guessed keywords:

| I wrote | som said |
|---|---|
| `var name = …` | "som declares variables with `let mut`, not `var`" + fix 👍 |
| `const name = …` | "som declares variables with `let`, not `const`" + fix 👍 |
| `let name = …` | works |

### 4. Joining strings / printing values

| I wrote | Result |
|---|---|
| `print("hello " + name)` | "cannot apply `+` to a string and a string — no such operation". **No hint about what to do instead.** |
| `print("hello", name)` | "`print` takes 1 argument, but 2 were given" |
| `print(f"hello {name}")` | "missing `,`", with a suggested fix of **`print(f,"hello {name}")`**. That suggestion is nonsense. Python's f-strings are common enough to deserve a "som has no f-strings" message. |
| `print("hello {name}")` | compiles, prints `hello {name}` literally |
| `print("hello ${name}")` | prints `hello ${name}` literally |

I gave up and called `print` twice.

### 5. Printing a number (the worst part of the session)

```som
println(a + b);
```
```
error: expected `cstr`, found an integer
   │ pub fn println(text: cstr) {
   │                      ──── `cstr` expected because of this
                                   /Users/lucas/som/std/io/io.som:3:22
```

**Thought:** What's a `cstr`? The error points into the std source, and that source shows
`c::printf("%s\n", text)`. Guesses that failed:

| Guess | Error |
|---|---|
| `str(a)` | "expected a value, found type `str`" |
| `a.to_string()` | "an integer has no method `to_string`" |
| `print_int(a)` | "cannot find function" |
| `io::println_int(a)` | "cannot find function" |
| `a as cstr` | "only numbers convert with `as`" |
| `format("{}", n)` | "cannot find function" |
| `println("{}", n)` | "takes 1 argument" |
| `n.str()` | **two** errors for one mistake ("expected identifier" + "found type") |

Then I copied what I saw in the std source:

```som
c::printf("%d\n", 42);
```
```
42
```

Works with no import. So the *real* way to print a number seems to be calling C directly. A beginner
needs either a `println` that takes any value, or an error that says "to print a number, use …".

### 6. 🐛 `print` and format specifiers

The std snippet in another error showed `print` calls `c::printf(text)`. So I tested:

```som
print("score: 100%d %s %s\n");
```
```
score: 1001829775952 � (null)
```

**Bug.** Junk memory. `println("100% done")` is fine because it uses `"%s\n"`. `print` should do the
same.

### 7. Typos and imports

- `prnitln("hi")` → "did you mean `println`?" + fix. 👍
- `use som::*;` → "expected identifier, found `*`". No hint that glob imports don't exist.
- `import io` → "expected ;" **and** "expected a function parameter name" on the *next* line. Very
  confusing. It deserves a "som writes `use`, not `import`" like the other keywords get.

### 8. Functions

| I wrote | som said |
|---|---|
| `def greet(name):` + indented body | "som declares functions with `fn`, not `def`" **and** "som puts a body in braces" + fix 👍 |
| `function greet(name) {` | "…with `fn`, not `function`" 👍 |
| `fn greet(name) { println(name); }` | **works.** Untyped parameters are inferred. Lovely for a beginner. |

Small rendering issue in the brace fix: the closing `}` line has no gutter or line number, and the fix
still shows `def` (each fix is applied to the original line on its own):

```
   2 + def greet(name) {
   3 +     println(name);
}
```

`return a + b;` and a tail expression `a + b` both work. `if (x == 5)` with parens works too.

### 9. Loops

| I wrote | som said |
|---|---|
| `for i in range(10)` | "som writes a range `0..10`: `range` is how Python writes it" + fix 👍 |
| `for (let i = 0; i < 3; i++)` | "som loops over a range: `for i in 0..n`…" + fix 👍 (the label blames `++`, but the problem is the whole C-style header) |
| `i += 1;` | **"expected expression"** pointing at `=`. Cryptic. `+=` is in almost every language; it should either work or get a "som has no `+=`: write `i = i + 1`" message, the way `++` does. |
| `i = i + 1;` on a `let` | "`i` is not mutable" + "make `i` mutable" fix 👍 |
| `i++;` | "som has no `++`: write `i = i + 1`" + fix 👍 |

### 10. FizzBuzz (success!)

```som
use som::io;
for i in 1..16 {
    if i % 15 == 0 { println("FizzBuzz"); }
    else if i % 3 == 0 { println("Fizz"); }
    elif i % 5 == 0 { println("Buzz"); }      // ← my mistake
    else { c::printf("%d\n", i); }
}
```
```
error: som writes `else if`, not `elif`   + fix
```

After the fix it printed FizzBuzz correctly. 🎉

### 11. Booleans

- `if x = 5` → "`=` assigns; to compare, use `==`" + fix 👍
- `and` works. `||` → "som writes `or`, not `||`". `not` → "som writes `!`, not `not`".

**Thought:** `and`/`or` are words but `not` is `!`? Python people will write `not`, and C people will
write `&&`/`||`. Each group gets one of the three wrong. Whichever way this goes, the mix is a surprise.

### 12. Lists

```som
let names = ["ann", "bob", "cid"];
for n in names { println(n); }        // works
names.len()                            // "it's a property" + fix → names.len  👍
```

- `let mut nums = []; nums.append(1); nums.push(2);` → three errors: "`[_; 0]` has no method `append`",
  the same for `push`, and "cannot infer the type". `[_; 0]` is jargon, and I learn that arrays are
  fixed-size only by working it out myself.
- `List()`, `Vec::new()`, `HashMap::new()` → "cannot find function".
- `use som::vec` / `list` / `collections` → "module `som::vec` not found, looked in `som/vec`".
  **No list of available modules.** Dead end: I couldn't find a growable list.

Indexing:

- `xs[5]` on a 3-element array → compile-time "index `5` is out of bounds, the array has 3 elements". 👍
- `xs[i]` with a variable → "an index has to be a number written out… use `xs.get(i)`". Surprising
  (can't index with a variable!?), but the hint got me unstuck.

### 13. Optionals

- `let n: i32 = xs.get(i);` → "expected `i32`, found `i32?`… check it first: inside `if v { … }`… or
  give a fallback: `v else 0`" + fix. 👍 Very clear.
- `v.unwrap()` → "`v` might be `none`" (same hints). 👍
- `null` → "som writes `none`" + fix. 👍
- This worked first try after reading those hints:

```som
let v = xs.get(i);
if v { c::printf("%d\n", v); } else { c::printf("nothing at %d\n", i); }
let w = xs.get(i) else -1;
```

🐛 **But:** `c::printf("%d\n", xs.get(1))` (passing the optional directly) compiles and prints `1`, and
`xs.get(9)` prints `0`. That's the presence flag, not the value. Nothing warned me.

### 14. Types and structs

```som
let x: int = 5;  let y: string = "hi";  let z: float = 1.5;
```

Each got "som writes `i32`/`str`/`f32 or f64`" + fix. 👍

Then:

```som
let y: str = "hi";
println(y);
```
```
error: C can't take a `str`
   │ println(y);
   │         ━ C needs text ending in a zero byte
     ┆ pass a `cstr`; a string literal becomes one
```

**Thought:** The compiler *told* me strings are `str`, and now its own `println` won't take one. I
never wrote C, so "C can't take" makes no sense to me. There's also no way to turn a `str` variable
into a `cstr`.

It gets stranger. The type of a string literal depends on what you do with it later:

```som
let a = "bob";
c::printf("%d\n", a.len);    // fine: a is str
```
```som
let a = "bob";
println(a);
c::printf("%d\n", a.len);    // error: `cstr` is not a struct, only structs have fields and methods
```

Adding a print line breaks a *different* line. (The message is also wrong: arrays have `.len` and
`.get`, so it isn't only structs.)

Strings can't be compared: `a == "bob"` → "no such operation". With `cstr` it says "implement `Eq` for
the type". `a.equals(...)` → no such method. `c::strcmp(a, "bob")` → "C can't take a `str`". I never
compared two strings.

Classes/structs:

| I wrote | som said |
|---|---|
| `class Dog { name: cstr  fn bark(self) {…} }` | "som declares structs with `struct`, not `class`" 👍 + "`self` only exists in methods, a method is written in an `impl` or `trait` block" 👍, plus two noisy follow-on errors (`missing }` with a fix of `name: cstr}`, "expected expression") |
| `Dog("rex")` | "cannot find function `Dog`". **No hint.** |
| `new Dog("rex")` | "som has no `new`: make a struct with `Dog { … }`, or … `Dog::new(…)`" 👍 That message should also fire for plain `Dog("rex")`. |
| `Dog { name: "rex" }` + `impl Dog { fn bark(self) … }` | works |
| `enum Color {…}; Color.Red` | "cannot find variable `Color`". It should suggest `Color::Red`. |
| `switch c { case Red: … }` | "som has no `switch`: it's `match x { 1 => …, _ => … }`" 👍 |
| `match` missing `Blue` | "`Blue` isn't matched" + fix. The suggested `Blue => {},` compiles and works. The fix rendering is broken, though: `}` lines have no gutter and the indentation is off: |

```
     ▪ add an arm for `Blue`
   6 -     Color::Green => println("green"),
}
   6 +     Color::Green => println("green"),
    Blue => {},
}
```

"this `Color` can be that" is also an odd phrase. Maybe "`c` can still be `Color::Blue`".

### 15. Ownership (the Rust-ish part)

```som
fn show(d: Dog) { println(d.name); }
fn rename(d: Dog) { d.name = "max"; }
let mut d = Dog { name: "rex" };
show(d); show(d); rename(d); println(d.name);
```

It took 4 rounds:

1. "`d` is not mutable" in `rename` → fix: `mut d: Dog`. (Python brain: I wanted the caller's dog
   renamed, but this fix just makes a local copy mutable. Not wrong, but not what I meant.)
2. "`d` is used after it was consumed" ×3. Good wording ("consumed here, gone after this point"). It
   hints `&d`, but there's no clickable fix, and the same three hint lines repeat three times.
3. I added `&d` at the call sites → "expected `Dog`, found `&Dog`" ×3. **No suggestion to change the
   parameter type to `&Dog`**, even though the error points right at it.
4. `d: &Dog` → "cannot assign through a `&` reference… needs a `&mut` reference". Then
   "expected `&mut Dog`, found `&Dog`" at the call site, again with no fix offered.

It worked in the end. Each message is correct, but none of them give the fix that finishes the job.
The "found `&Dog`, expected `Dog`" case in particular could offer "change the parameter to `&Dog`".

### 16. More CLI

```
$ som check h1.som        → "checked: `h1.som`", exit 0
$ som check o2.som        → errors, "could not compile `o2.som`", exit 1
$ som build h1.som        → "built: `h1`", produces ./h1 (works)
$ som run a1.som 2>/dev/null   → only "hello world": status lines go to stderr 👍
```

Small things:

- `som check` says "could not **compile**", which is odd for a command that doesn't compile.
- `run` says "built: `h1.som`" but `build` says "built: `h1`". Inconsistent subject.

Empty directory:

```
$ mkdir proj && som run proj
error: the program has an no entrypoint 
     ┆ welcome to som!
     ┆ to get started, write your first lines in a `.som` file, like `println("Hello, world!");`
     ┆ happy hacking :)
```

Nice and friendly, but: "an no" typo, a trailing space, and the example doesn't compile without
`use som::io;`.

### 17. `main` and arguments

- `fn main() { … }` plus a top-level `println(...)` →
  "`main` is already defined with the same signature… and is defined again here" with the **`println`
  line** marked as the second definition. Very confusing. It should say something like "top-level code
  becomes `main`, so you can't have both".
- `fn main() -> i32 { return 3; }` → exit code 3. 👍
- `fn main(args)` → "cannot index an unknown type… add a type annotation".
- `fn main(args: [cstr])` → "a slice is borrowed, write it `&[T]`". OK.
- 🐛 `fn main(args: &[cstr])` →

```
error: the C compiler rejected the generated code
     ┆ this is a bug in the som compiler
     ! …program.c:52:14: error: too few arguments to function call, single argument 'args' was not specified
     !    52 |     som_main();
```

Credit where due: the internal-error message is honest and keeps the generated C for debugging.

- `use som::env` / `os` / `args` / `sys` → not found. I couldn't read program args at all.

### 18. Runtime behaviour

| Program | Result |
|---|---|
| `10 / 0` (via variables) | 🐛 prints `-85999232`, exit 0. No crash, no error. |
| `2147483647 + 1` (i32) | prints `-2147483648`. Silent wraparound. |
| `fib(20)` recursive | `6765` 👍 |
| `1.5 * 2` | "cannot apply `*` to a float and an integer". No hint to write `2.0`. |
| `let unused = 5;` | no warning |
| `let x = 1; let x = x + 1;` | shadowing works |

### 19. Syntax-error recovery

Unclosed brace:

```som
fn f() {
    if true {
        println("x");
}            // ← the `if` is closed, the fn isn't
f();
```
```
error: missing `}`   at 6:5 (end of file)
     ▪ add the missing `}`
   6 + f();}
```

The fix is technically valid but clearly wrong: it moves `f();` inside the function. The indentation
makes it obvious the brace is missing after line 4.

Unterminated string:

```som
println("unterminated);
println("ok");
```
```
error: missing `,`   at 3:10
     ▪ add the missing `,`
   3 + println(",ok");
error: expected `)`, found `"`
```

**Bad.** The real problem (no closing quote on line 2) is never mentioned, and the suggested fix is
nonsense. The lexer should catch strings that run across a newline.

### 20. Things I never figured out

- Printing a number with som's own API (only `c::printf` worked)
- Joining two strings, or comparing two strings
- A growable list / vector / map
- Reading user input (`input()`, `io::read_line()`: not found)
- Reading command-line args (compiler bug)
- What `str` vs `cstr` means and how to convert between them

---

## Suggestions, in priority order

1. **Fix `print` to use `"%s"`.** It's a memory-safety bug in the most basic function.
2. **Fix `main(args: &[cstr])` codegen.**
3. **Make `println`/`print` accept integers (or anything printable)**, or add a clear "to print a
   number, use X" message where it fails today. Today every beginner has to find `c::printf`.
4. **Explain or hide `str` vs `cstr`.** At minimum, don't let a literal's type flip based on later use,
   and don't say "C can't take" to people who didn't write C. Suggest the conversion.
5. **When a module isn't found, list the ones that exist** (`available in som: io, …`). Same for
   "cannot find function" on a module path: list what *is* in `som::io`.
6. **Trap division by zero** (or at least in debug builds).
7. **Reject (or convert) som optionals passed to C variadics.**
8. Add the missing "som writes X" messages: bare `x = 1` (→ `let`), `import` (→ `use`), `+=`, f-strings,
   `Dog(...)` (→ `Dog { … }`), `Color.Red` (→ `Color::Red`), `: i32` return type (→ `-> i32`; today it
   says "missing `{`"), trailing `;` on a tail expression (→ "remove the `;`"; today it gives two
   errors), `"a" + b` and `a == b` on strings (→ point to whatever the real API is), `1.5 * 2` (→ `2.0`).
9. Offer a fix for "expected `T`, found `&T`" that changes the parameter type.
10. Detect unterminated strings in the lexer; anchor "missing `}`" to where indentation says it should
    go.
11. Fix multi-line fix rendering (lines with no gutter, wrong indentation).
12. Fix the welcome-screen example (`use som::io;`) and the "an no entrypoint " typo.
13. Pick a consistent style for `and`/`or`/`not` vs `&&`/`||`/`!`.
