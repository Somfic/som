# UX test: a Python-only newbie tries som

Persona: I only know Python. No README, no std source. I learn from `som --help` and the compiler's error messages.
Compiler: `som 0.1.0` (`target/debug/som`, branch `2027`, 2026-11-15). Every snippet below was really run; the output has been trimmed.

---

## TL;DR

**What went well**
- The "Python-ism → som" hints are great when they exist: `var` → `let mut`, `class` → `struct`, `True` → `true`, `None` → `none`, `List::new` → `List::empty/from/of/with_capacity`, missing `mut` → "make `i` mutable", `cout` → "did you mean `count`?".
- The borrow and move errors are very readable ("consumed here, gone after this point", "tried to use here, but it's gone").
- The optional-type errors teach the language: "check it first: inside `if x { … }`… or give a fallback: `x else …`".
- `som run nope` / `som run hello` give helpful file errors ("did you mean `hello.som`?").
- Top-level statements just work, like a Python script. Untyped `fn greet(name)` also just works.

**What went badly**
1. 🔴 **The compiler hangs forever** on `print(str(5));`, `print(a.cstr);`, `foo(i32);` and `let x = [str];`. A primitive type name used as a value inside `(...)` or `[...]` makes the parser loop. `som check` hangs too. As a newbie I'd think my computer froze.
2. 🔴 **I can't print a number or a `String` with `print`.** `print` only takes `cstr`, and no error tells me how to turn an int or a `String` into one. I only got going by copying `c::printf` out of a std snippet shown in an error. That is a private-looking escape hatch, and it's unchecked.
3. 🔴 **`c::printf` is silently unsafe.** `c::printf("%s\n", a_String)` prints garbage bytes. `c::printf("%d", some_optional)` prints `88691876`. There's no warning in either case.
4. 🔴 **Runtime errors are silent.** `10 / 0` printed `-185337728`. `i32::MAX + 1` wrapped. Python would raise `ZeroDivisionError`.
5. 🟠 **No Python dict that works with string keys.** `Map<cstr,_>` and `Map<String,_>` fail with "doesn't implement `Eq`/`Hash`", and that error is repeated once per use (7 errors in total).
6. 🟠 **Hints are inconsistent.** `var` and `class` get a hint, but `def`, `function`, `func`, `elif`, `not`, `#` comments, `'single quotes'`, `x += 1`, `x++`, `len(x)`, `range(n)`, `input()` and `str(x)` don't. Some produce baffling errors ("expected a semicolon" pointing at a function name).
7. 🟠 **The `mut self` suggestion leads into a trap.** It makes the method consume the object. The correct fix (`&mut self`) is never suggested.
8. 🟡 `&&` is not logical AND, but nothing says so: `x == 3 && x < 5` → "cannot apply `==` to an integer and `bool`".
9. 🟡 Discoverability: there's no way to ask "what's in `som::io`?" or "what methods does `String` have?". Wrong function names (`io::prnt`) get no "did you mean", but wrong variable names do.

---

## The session, step by step

### 0. Finding the tool

```
$ som
The som compiler
Usage: som <COMMAND>
Commands: run, build, check, lsp, help
A <PATH> is a .som file, or a directory of them.
```
👍 Clear. `som run file.som` seems to be the equivalent of `python file.py`. It's slightly odd that running bare `som` exits with code 2 rather than just showing help.

### 1. Hello world, Python style

```python
print("Hello, world!")
```
```
error: cannot find function `print`
  ┆ `print` is defined in module `som::io` but is not accessible from here. try `som::io::print` or `use som::io;`
  ▪ change to `som::io::print`
  ▪ add `use som::io;`
```
👍 Excellent. I added `use som::io;` and it worked. Missing semicolons were OK for this one-liner.

**Surprise:** `print` doesn't add a newline (`Hello, world!exit=0`). I eventually guessed `println`, and it exists.

### 2. Variables, f-strings

```python
name = "Bob"
print(f"Hi {name}, you are {age}\n")
```
- `expected ;` after `name = "Bob"`. Fine: this is a semicolon language.
- The f-string error is **bad**: "missing `,`… add the missing `,`: `print(f,"Hi …")`". That suggestion is nonsense. It should say "som has no f-strings".
- After adding `;`: `cannot find variable name`. **There's no hint that I need `let`.**
- I guessed `var name = …;`, and this got the great hint `som declares variables with let mut, not var`. It's a shame I had to type `var` (a JS word) to get it. Plain `name = ...` should get the same hint.

### 3. Strings

```
let name = "Bob";
print("Hi " + name);
error: cannot apply `+` to a string and a string
```
😕 There's no hint about how to concatenate. Later I found `String::from("a") + String::from("b")` compiles, but I could never print the result (see §5).

Also confusing: the error says "a string", but `print` wants "`cstr`". Are string literals `cstr`? Is `str` a type too? `let s: str = "hi";` compiles. There are three string-ish names and I don't know which one is which.

### 4. Functions

| I wrote | Got |
|---|---|
| `def add(a, b):` | `expected ;` pointing at `add` 🤯 |
| `function add(a, b) {` | `expected ;` pointing at `add` |
| `func add(a, b) {` | `expected ;` pointing at `add` |
| `fn add(a, b) {` | ✅ |

`var` and `class` get "som uses X, not Y", but `def`/`function`/`func` don't. As a Python user, `def` is the **first** thing I'd type. Please add `def/function/func → fn`.

Type errors on calls are good (`add takes 2 arguments, but 1 was given`). `not every path returns i32 … this end can be reached without a value` is also good.

### 5. Printing a number (the wall)

```
print(i);           → expected `cstr`, found an integer (and it shows std's `pub fn print(text: cstr)`)
print(str(5));      → 🔴 COMPILER HANGS (also `som check`)
print(5.to_string())→ an integer has no method `to_string`
io::print_int(5)    → cannot find function
print(5 as cstr)    → only numbers convert with `as`
print("%d", 5)      → print takes 1 argument
```
After about six attempts I copied `c::printf(text)` from the std snippet shown in the type error. `c::printf("%d\n", 5);` works without any `use`. This is the only way I found to print a number, and it isn't something the compiler suggested.

For a `String`, I never found a working route:
```
c::printf("%s\n", a);   // a: String → prints "��" with no warning
print(a.cstr);          // 🔴 HANGS
a.as_cstr() / a.c_str() / a.to_cstr()  → no such method
```
**Suggestion:** when `print` gets a non-`cstr`, say how to print it ("use `println(x.fmt())`" or whatever the real answer is). Better still, make `print` accept anything printable.

### 6. The hang (minimal repros)

All of these hang `som run` / `som check` forever with no output:
```
print(str(5));
print(a.cstr);
foo(i32);
let x = [str];
```
But `str(5);` at the top level gives "expected an expression", and `let q = a.cstr;` gives "expected identifier, found `cstr`". It looks like error recovery inside call args or list elements doesn't advance past a primitive-type keyword.

### 7. Control flow

| I wrote | Result |
|---|---|
| `for i in range(5):` | `missing {` 👍 clear |
| `range(5)` | `cannot find function range`. No hint; I guessed `0..5`, which works |
| `if x > 5:` | `missing {` 👍 |
| `elif` | `expected ;` at the condition, then `expected expression` at `else` 😕. `else if` works |
| `and` | ✅ works, nice |
| `or` | ✅ works |
| `not (x == 3)` | `cannot find function not`. No hint that it's `!` (`!` works) |
| `x == 3 && x < 5` | `cannot apply == to an integer and bool` 🤯. Is `&&` a precedence thing? There's no hint to use `and` |
| `i += 1` | `expected expression` at `=` 😕. Is `+=` not supported? There's no hint |
| `i++` | `expected expression` |
| `i = i + 1` on `let i` | 👍 "make `i` mutable" with a fix |
| `while` | ✅ |
| `# comment` | `expected expression` with no hint that comments are `//` |
| `'single'` | `expected expression` with no hint to use double quotes |

FizzBuzz eventually worked first try once I knew `let mut`, `else if`, `c::printf` and `i = i + 1`.

### 8. Lists

```
let nums = [1, 2, 3];              ✅ (it's a `[i32; 3]` array)
nums.append(4);                    → `[i32; 3]` has no method `append`. No hint to use a List
List::new()                        → 👍 "`List` has no `new`; use List::empty, List::from, List::of or List::with_capacity"
List::of(1, 2, 3)                  → takes 1 argument. Which argument? The signature isn't shown (it was for `print`)
List::of([1, 2, 3])                ✅
nums[5]                            → "only an array's length is known, so only arrays are indexed; use nums.get(5)" — good hint, odd reasoning for a newbie
for n in nums {…} then nums.len    → 👍 great move error, suggests `&nums`
nums.get(5) printed with %d        → prints `0`, no error
```
Optionals:
- `x.unwrap()` → "`x` might be none; check it first: inside `if x { … }`… or `x else …`" 👍 very teachable
- `if x == None` → 👍 "som writes None as none"
- after `if x != none`, `let y: i32 = x;` → "expected i32, found &i32; dereference it: `*x`" 👍, though `&` and `*` are alien to me
- `ages.get(&k) else 0` → "borrow it: `else &(0)`". It works, but `&(0)` is ugly; `&0` works too.

### 9. Dicts

```
{"bob": 42}                  → "expected ; or }" (no "som has no dict literals" hint)
Dict::empty()/HashMap::empty → cannot find (no "did you mean Map?")
Map::empty()                 → cannot infer type; "add a type annotation" (but `let m: Map = …` gives the same error; no example of `Map<K, V>`)
Map<cstr, i32>               → 7 errors: cstr doesn't implement Eq / Hash, once per call
Map<String, i32>             → String doesn't implement Eq (×5)
Map<i32, i32>                ✅ works
m["bob"] = 42                → first error is "expected usize, found a string", then "only arrays are indexed"
```
🟠 As a Python user, dicts with string keys are the #1 data structure, and I couldn't make one. The Eq/Hash errors should be reported once, not once per use.

### 10. Classes

- `class Dog:` → 👍 "som declares structs with struct, not class"
- a method inside `struct {}` → "`self` only exists in methods; a method is written in an `impl` or `trait` block" 👍 (but the first error, "expected a struct field name", is noise)
- `fn birthday(self) { self.age = … }` → suggests **`mut self`**. I followed it, and then got two "`d` is used after it was consumed" errors on `d.bark()` / `d.birthday()`. 🟠 The suggestion misled me. The right fix was `&mut self` and `let mut d`, and I only got there by copying `&self` from a std snippet.
- With `&self` / `&mut self`, the errors are good ("cannot assign through a & reference; changing it needs a &mut reference", "make d mutable").

### 11. Numbers & runtime

```
a + b  (i32 + float)     → "cannot apply + to an integer and a float" (no hint to use `a as f64`, which works)
10 / 0                   → prints -185337728, exit 0 🔴
i32 max + 1              → wraps to -2147483648 silently
```

### 12. Input

`input("name? ")` → cannot find function. No hint. I couldn't find a way to read user input.

### 13. CLI odds and ends

- `som check` → `checked: u1.som` 👍; `som build` → `built: u1` (no `.som`), while `som run` prints `built: u1.som`. Minor inconsistency.
- `som run` prints `built: …` before the program's output. That's noisy for a script runner; Python prints nothing.
- `-v` shows the `cc` command: interesting, but nothing a newbie needs.
- Error locations are right-aligned on a separate line (`hello.som:1:1`). It took me a moment to spot them.

---

## Suggested fixes, by impact

1. Fix the parser hang on a type keyword in call or list position (`foo(i32);`).
2. Give newcomers a printing story: make `print`/`println` accept ints, bools and `String`, or have the error say exactly how to convert.
3. Make `String` implement `Eq` + `Hash` (and/or `cstr`) so `Map<String, _>` works, and dedupe trait-bound errors.
4. Type-check `c::printf` varargs, or at least warn when you pass a `String`, an optional or a reference.
5. Add more "som uses X, not Y" hints: `def`/`function`/`func`→`fn`, `elif`→`else if`, `not`→`!`, `#`→`//`, `'…'`→`"…"`, `+=` (if unsupported, say so), bare `x = …`→`let`, f-strings, `range(n)`→`0..n`, `len(x)`→`x.len`, `str(x)`, `input`, `Dict`/`HashMap`→`Map`, `&&`→`and`.
6. When suggesting `mut self` for a field assignment, suggest `&mut self` instead.
7. Add "did you mean" for functions and paths (`io::prnt`), not just variables, and for wrong arity show the callee's signature (as `print` already does).
8. Trap division by zero at runtime with a clear message.
