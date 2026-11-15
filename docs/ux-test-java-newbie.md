# UX test: a Java-only newbie tries som

Persona: I only know Java. No README, no std source. I learn from `som --help` and the compiler's error messages.
Compiler: `som 0.1.0` (`target/debug/som`, branch `2027`, 2026-11-15). Every snippet below was really run (with a 10s timeout wrapper); output is trimmed.

---

## TL;DR

**What went well**
- Some Java-isms get great hints: `class` → `struct`, `var` → `let mut`, `null` → `none`, `function`/`method`/`def` → `fn`, `int a` in a parameter list → "som writes a parameter's name first, then its type: `a: int`", missing `mut` → "make `p` mutable", `List::new` → `List::empty/from/of/with_capacity`, `.len()` → "it's a property, read it without parentheses".
- `println("x")` with no import tells me exactly where it lives and offers both fixes (`som::io::println` or `use som::io;`).
- Borrow and move errors are readable. Combined with "have the method take `self: &…` instead" and "changing it needs a `&mut` reference", I got a working mutable class (`Account` with `deposit`/`withdraw`) without any docs.
- Compile-time array bounds check (`arr[10]` → "the array has 3 elements") is a nice surprise.
- Field errors list the available fields (`available fields: bytes; properties: len`).
- `som run Bank.java` → "create `Bank.som`…" is friendly.

**What went badly**
1. 🔴 **I couldn't discover the integer type.** `int`, `Integer`, `long`, `double`, `boolean`, `byte` all say "cannot find type `int`" with no hint. Type errors say "found an integer" and never name the type. I only got `i32` by guessing from Rust. A real Java newbie would be stuck at step 3.
2. 🔴 **I couldn't print a `String` at all.** `println` takes `cstr`. `String → cstr` has no discoverable path (`.toString()`, `.cstr()`, `as cstr`, `.c_str()` all fail). `printf("%s\n", aString)` prints garbage (`�B`) silently.
3. 🔴 **`printf` is a silent footgun.** `printf("%s\n", 42)` crashes with exit 1 and **no output at all**, not even earlier `printf("before\n")` lines. `printf("%d\n", xs.get(1))` printed `1` instead of `20`. `printf("%d\n", xs.get(1) else &0)` printed `9966756`. None of these get a warning.
4. 🔴 **Strings can't be compared or concatenated.** `a == b` on two `String`s → "implement `Eq` for `String`" and the suggested fix generates an `impl Eq for String` with an empty body *in my file*. `+` does the same with `Add`. As a Java dev, `.equals()` and `+` are day-one stuff.
5. 🟠 **Methods/`impl` were undiscoverable.** Putting `fn` inside a `struct` → "expected a struct field name". `interface`, `implements`, `extend`, `fn Person.getAge()` give no hint. I only got `impl` by guessing from Rust. `this` → "cannot find variable `this`" (no "som calls it `self`"). Using `self` without a `self` parameter gives no "add `self` as first parameter" hint either.
6. 🟠 **Struct field hints are wrong.** `String name;` inside a struct suggests `String: name;`. `private String name;` suggests `private: String name;`. `public static void main` suggests `public: static void …`. Following any of these makes it worse. (The parameter-list version of the same mistake gets the *correct* hint.)
7. 🟠 **`&&` and `||` don't exist and nothing says so.** `x == 3 && x < 5` → "cannot apply `==` to an integer and `bool`". `(a) && (b)` → "cannot apply `&` to `bool` and `bool`". `||` → "expected an expression". I found `and`/`or` by luck. (`!` works but `not` doesn't, which is inconsistent with `and`/`or`.)
8. 🟠 **Indexing a list/array with a variable is forbidden.** `arr[i]` → "an index has to be a number written out", use `arr.get(i)`. That's a big shock for a Java programmer, since `for (int i…) arr[i]` is how I write every loop. The message is clear, though.
9. 🟡 **`use som::io;` imports the module's *contents*, not the module.** After `use som::io;`, `io::println(...)` → "cannot find function `io::println`" with no hint. Java's `import` doesn't work like either, but I expected the name `io` to be usable.
10. 🟡 **Unknown *method* errors never list methods or suggest near-misses.** `getNam()` doesn't suggest `getName`. `length()`/`size()` don't suggest `len`. `add()` on a List doesn't suggest `push`. (Unknown *function* `printInt` does suggest `println`, and unknown *fields* list the fields.)
11. 🟡 **Hints suggest editing the std library.** `s.bytes` → "make it `pub`" with a diff against `std/data/string.som`. `==`/`+` on `String` → "implement `Eq`/`Add` for `String`".
12. 🟡 Missing hints for: `int x = 5;`, `final`, `x++`, `x += 1`, `i++` in for loops, C-style `for (;;)`, `for (x : xs)` (this one is okay: "missing `in`"), `new Person(...)`, `Person.new(...)`, `Person(...)`, `/* */` comments, `'a'` chars, `?:` ternary, `try/catch`, `throw`, `switch/case`.
13. 🟡 Error locations: `/* hi */` and `# hi` report "expected an expression" at the **end of the previous line**. `public class Main` reports "expected ;" on `Main` *and* the good `class` → `struct` hint for the same token (duplicate, and the bad one comes first).
14. 🟡 `10 / 0` prints `-85999232`. No runtime error.
15. 🟡 `som run bank` on macOS (case-insensitive FS) picks up the **built binary** `Bank` and says "could not read `bank`: stream did not contain valid UTF-8".

---

## The session, step by step

### 0. Getting started

```
$ som --help
Commands: run, build, check, lsp, help
A <PATH> is a .som file, or a directory of them.
```
Clear enough. `som run --help` mentions `--emit tokens,ast,mir,c`, which I don't understand, but OK.

### 1. Hello World, the Java way

```java
public class Main {
    public static void main(String[] args) {
        System.out.println("Hello, World!");
    }
}
```
```
error: expected ;                         ← on `Main`
error: som declares structs with `struct`, not `class`   ▪ change to `struct`
error: missing `:`                         ← on `static`
     ▪ add the missing `:`
   2 +     public: static void main(String[] args) {
```
> 🤔 The `class` hint is great. But why does the first error say "expected ;" on the same word? And `public: static void`?? That can't be right.

Changed to `struct` → same "expected ;" + `public:` hint. Removed the class entirely:

```java
public static void main(String[] args) { System.out.println("Hello, World!"); }
```
→ 6 errors. One suggests `main(String[], args)`. Another: `expected identifier, found 'out'`, so `out` seems to be a keyword?

> 🤔 Nothing tells me `public`/`static`/`void` aren't a thing, or what a function looks like.

`void main() { ... }` → still "expected ;" on `main`. Plain `System.out.println("Hello, World!");` at top level → just the `out` error. Then I tried bare `println(...)`:

```
error: cannot find function `println`
     ┆ `println` is defined in module `som::io` but is not accessible from here.
       try `som::io::println` or `use som::io;`
```
> 😀 Excellent hint. Both fixes worked:

```
$ som run Main.som          # use som::io; println("Hello, World!");
Hello, World!
```
Then I tried the Java-ish "qualified" form, `use som::io; io::println(...)`:
```
error: cannot find function `io::println`
```
> 🤔 So `use som::io` dumps everything into scope, like `import som.io.*`. OK, but there's no hint.

### 2. Functions

```java
int add(int a, int b) { return a + b; }
```
→ "expected ;" + two bad "missing `,`" suggestions (`int add(int, a, int b)`). No hint about how to declare a function.

Trying keywords from other languages:
| tried | result |
|---|---|
| `function add(a, b)` | ✅ "som declares functions with `fn`, not `function`" |
| `method add(a, b)` | ✅ same hint |
| `def add(a, b)` | ✅ same hint |
| `func add(a, b)` | ❌ "expected ;" |
| `int add(...)` (Java) | ❌ "expected ;" |

> 🤔 The Java form is the one I'd actually write, and it's the one without a hint.

`fn add(int a, int b) int {` →
```
error: som writes a parameter's name first, then its type   ▪ change to `a: int`
error: missing `{`   ← on the return type `int`
```
> 😀 Great hint for params. The return type is unclear: `: int` → "missing `{`". I guessed `-> int` (from Java lambdas, sort of).

`fn add(a: int, b: int) -> int` →
```
error: cannot find type `int`   (×3, no hint)
```
| tried type | result |
|---|---|
| `int`, `Integer`, `long`, `double`, `boolean`, `byte` | "cannot find type", **no hint** |
| `String` | ✅ "defined in `som::data` … try `use som::data;`" |
| `char` | ✅ exists |
| `i32` | ✅ exists (**guessed from Rust**) |

I tried to make the compiler tell me the type name: `let y: bool = 5;` → "expected `bool`, found **an integer**". `5.foo()` → "**an integer** has no method `foo`". It never says `i32`.

> 😩 This is the biggest wall so far. A pure Java dev stops here.

`int x = 5;` → "expected ;" (no "use `let`" hint, though `var x = 5;` → "use `let mut`" ✅).

### 3. Printing a number

`println(add(2, 3))` → "expected `cstr`, found an integer" (it shows me `std/io/io.som:3` where `println(text: cstr)` is defined; nice context).

| tried | result |
|---|---|
| `"sum: " + x` | cannot apply `+` to a string and an integer |
| `x.toString()` / `x.to_string()` | an integer has no method … |
| `String.valueOf(x)` | cannot find variable `String` |
| `x as cstr` | only numbers convert with `as` |
| `x.str()` | "expected identifier, found `str`" + "this is a type, where a value goes" |
| `printInt(x)` | ✅ "did you mean `println`?" (not helpful, but nice it tries) |
| `printf(x)` | expected `cstr`… **but it shows me `extern { pub fn printf(pattern: cstr, ...) -> i32; }`** |

> 😀 Oh, `printf` exists, like `System.out.printf`!

```
printf("2 + 3 = %d\n", add(2, 3));    → 2 + 3 = 5   ✅
```
Then I made a typo-ish mistake:
```
printf("before\n");
printf("%s\n", 42);
printf("after\n");
```
```
$ som run Main.som
built: `Main.som`
[exit=1]
```
> 😱 No output at all, not even "before". No error message, just exit 1. In Java I'd get an `IllegalFormatConversionException` with a stack trace. I'd have no idea what happened here.

### 4. Variables and loops

```java
for (int i = 0; i < 5; i++) { ... }
```
→ "expected identifier, found `(`, **expected a name for each value**". That hint is vague, but it suggests a for-each.

`while (i < 5) { ...; i++; }` → "expected expression" at `;` after `i++` (no "som has no `++`" hint).
`i += 1;` → "expected expression" (no hint).
`i = i + 1;` → ✅ "`i` is not mutable" ▪ "make `i` mutable: `let mut i = 0;`". 😀

`for i in 0..3` (guessed) ✅. `for (i in 0..3)` → same vague error. `for x : nums` → ✅ "missing `in`: expected `in` and what to loop over". 😀

`if (i == 3) {…} else if (i > 3) {…} else {…}` ✅ works with Java parens. `let y = if x > 2 { 1 } else { 0 };` ✅. `match x { 1 => …, _ => … }` ✅ (guessed).

### 5. Classes

```java
struct Person {
    private String name;
    private int age;
    public Person(String name, int age) { this.name = name; ... }
    public int getAge() { return this.age; }
}
Person p = new Person("Bob", 30);
```
```
error: missing `:`  ▪ add the missing `:`
   4 +     private: String name;
```
Dropped `private`: `String name;` →
```
     ▪ add the missing `:`
   2 +     String: name;
```
> 😩 This is backwards. For *parameters* the compiler knows to say "name first, then type". For *fields* it suggests `String: name`.

`name: String;` → ✅ "items are separated with `,`, not `;`", plus a second cascade suggestion `age: i32};`.

Methods inside the struct → "expected identifier, found `fn`: expected a struct field name". No hint about where methods go.

| tried | result |
|---|---|
| `fn Person.getAge()` / `fn Person::getAge()` | "missing `(`" |
| `implements Person { }`, `extend Person { }`, `interface Shape { }` | "expected ;" |
| `impl Person { fn getAge() -> i32 { return this.age; } }` | **guessed from Rust** → "cannot find variable `this`" (no hint) |

`self.age` → "cannot find variable `self`" (no "add a `self` parameter" hint). Construction:
- `new Person("Bob", 30)` → "expected `;`, found identifier" (no hint about `new`)
- `Person("Bob", 30)` → "cannot find function `Person`" (no struct-literal hint)
- `Person { name: "Bob", age: 30 }` ✅ (guessed)

With `fn getAge(self)`, calling it twice:
```
error: `p` is used after it was consumed
     ┆ `p` is a `Person`, which moves rather than copies
     ┆ or, if a reference is enough where it's consumed, borrow it there: `&p`
```
> 🤔 A getter *consumes* my object?! Very weird for Java. I followed the hint literally: `&p.getAge()` → same error (precedence). `(&p).getAge()` →
```
error: the method takes the `Person` itself, but this is a reference to one
     ┆ have the method take `self: &…` instead
```
> 😀 That's the real fix. It should have been in the *first* error.

`fn birthday(self: &Person) { self.age = … }` → "cannot assign through a `&` reference: changing it needs a `&mut` reference" ✅ → `self: &mut Person` → "`p` is not mutable ▪ `let mut p`" ✅ → works, prints `31`. 😀

Static constructor: `Person.new(...)` → "cannot find variable `Person`" (no "use `Person::new`" hint). `Person::new(...)` ✅.

### 6. Strings

`fn getName(self: &Person) -> String { return self.name; }` → "cannot move `self.name` out of a reference: borrow it instead, with `&self.name`" ✅ (but it only appears after every other error is fixed).

Printing a String:
| tried | result |
|---|---|
| `println(name)` | expected `cstr`, found `String` |
| `name.cstr()` | expected identifier, found `cstr` + "this is a type" |
| `name.toString()`, `.c_str()`, `.as_cstr()` | has no method … |
| `name as cstr` | only numbers convert with `as` |
| `printf("%s\n", q.getName())` | **prints `�B`** silently |

> 😩 I never managed to print a name. Garbage output without any warning is the worst kind of error.

Discovering String's API: unknown *fields* list what's there (`available fields: bytes; properties: len`). Unknown *methods* list nothing. `.length()` / `.size()` don't suggest `len`; `.len()` → ✅ "it's a property, read it without parentheses". `s.bytes` →
```
error: `bytes` is private to `String`
     ▪ make it `pub`
   2 +     pub bytes: List<u8>          ← in std/data/string.som!
```

Concatenation and comparison:
```
let t = s + " there";
error: cannot apply `+` to `String` and a string
     ┆ to give it `+`, implement `Add` for the type
     ▪ implement `Add` for `String`
   3 - }
   3 + }
impl Add for String {
    fn add(self: &String, other: &String) -> String {
    }
}
```
> 🤔 I'm supposed to implement string concatenation myself? Also, that diff looks broken (it changes `}` to `}` and the rest has no gutter).

`a == b` on Strings → same, with `Eq`. `a.equals(b)` → no method.

### 7. Arrays and lists

- `int[] arr = {1, 2, 3};` → 3 parse errors, no hint.
- `let arr = [1, 2, 3]; arr.length` → "`[i32; 3]` is not a struct, only structs have fields and methods". (First time I see `i32`! But then how do I get the length? `arr.len` turns out to work.)
- `arr[10]` → ✅ compile-time "index `10` is out of bounds, the array has 3 elements". 😀
- `arr[i]` → "an index has to be a number written out ┆ use `arr.get(i)`, which gives an optional". Clear, but surprising.
- `new ArrayList<i32>()` → "expected `;`". `ArrayList::new()` → not found (no hint toward `List`). `List::new()` → ✅ "use `List::empty`, …".
- `xs.add(1)` → no method (no "did you mean `push`"). `xs.push(1)` (guessed) ✅.
- `xs[0]` → ✅ "use `xs.get(0)`".
- `printf("%d\n", xs.get(1))` → prints **`1`** (should be 20). Silent.
- `xs.get(1) else 0` → "expected `&i32`… ▪ borrow it: `&0`" → `printf("%d\n", xs.get(1) else &0)` prints **`9966756`**. Silent again.
- `let v = xs.get(1) else &0; printf("%d\n", *v);` → `20` ✅ (guessed `*` from C).
- `for x in xs { … }` ✅.

### 8. null, operators, misc

- `let p: String = null;` → ✅ "som writes `null` as `none`" → but then `let p: String = none;` → "expected `String`, found `_?`" **and** "cannot infer the type of this expression". The hint led me to another error; it should say `String?`.
- `x == 3 && x < 5` → "cannot apply `==` to an integer and `bool`". 😩 That's baffling.
- `(x == 3) && (x < 5)` → "cannot apply `&` to `bool` and `bool`". So `&&` is two `&`s?
- `x == 1 || x == 3` → "expected an expression".
- `and` / `or` ✅ (guessed). `!(…)` ✅. `not (…)` → "cannot find function `not`".
- `x > 2 ? 1 : 0` → "expected `;`, found `?`".
- `'a'` → "expected expression".
- `// comment` ✅. `/* comment */` and `# comment` → "expected an expression" **pointing at the end of the previous line**.
- `try { } catch (Exception e) { }` → "expected ;" + `catch (Exception, e)` suggestion.
- `throw new RuntimeException("bad")` → "expected `;` or `}`".
- `switch (x) { case 1: … }` → 3 "expected ;" errors.
- `10 / 0` → prints `-85999232`.

### 9. Something that works

After all that, these ran correctly first try:

```som
use som::io;

fn fizzbuzz(n: i32) {
    for i in 1..n {
        if i % 15 == 0 { println("FizzBuzz"); }
        else if i % 3 == 0 { println("Fizz"); }
        else if i % 5 == 0 { println("Buzz"); }
        else { printf("%d\n", i); }
    }
}
fizzbuzz(16);
```
```som
use som::io;

struct Account { balance: i32 }

impl Account {
    fn new() -> Account { return Account { balance: 0 }; }
    fn deposit(self: &mut Account, amount: i32) {
        if amount <= 0 { println("invalid amount"); return; }
        self.balance = self.balance + amount;
    }
    fn withdraw(self: &mut Account, amount: i32) -> bool {
        if amount > self.balance { return false; }
        self.balance = self.balance - amount;
        return true;
    }
}

let mut acc = Account::new();
acc.deposit(100);
acc.deposit(-5);
if !acc.withdraw(500) { println("insufficient funds"); }
acc.withdraw(30);
printf("balance: %d\n", acc.balance);
```
```
invalid amount
insufficient funds
balance: 70
```
> 😀 Once I know the shape, it reads fine. Top-level statements are nice, with no `public static void main` ceremony.

### 10. CLI

- `som` (no args) → prints help, exit 2. Fine.
- `som run Bank.java` → ✅ "`Bank.java` does not exist ┆ create `Bank.som`, or a directory `Bank/` with a `main.som`".
- `som build Bank.som` → `built: Bank`, binary next to the source.
- `som run bank` (after building) → "could not read `bank`: stream did not contain valid UTF-8". macOS is case-insensitive, so it found the binary `Bank`. Confusing.
- `som check .` with `Main.som`, `Bank.som` and `Fizz.som` in the folder → only reported `Main.som`'s error. It's unclear whether the other files were checked at all (each has its own top-level statements).

---

## Suggested fixes, by impact

1. **Name the primitive types.** Hint `int`/`long`/`double`/`boolean`/`byte` → `i32`/`i64`/`f64`/`bool`/`u8`. Say "`i32`" instead of "an integer" in type errors, at least once.
2. **A way to print anything.** `println` for ints/`String`, or a hint from `expected cstr, found String/integer` to whatever the right conversion is.
3. **Type-check `printf` format strings** (or at least warn on non-literal or mismatched args), and report crashes ("segmentation fault") instead of a bare exit 1. Flush stdout before dying, if possible.
4. **`==` and `+` on `String`** should work from std, and hints shouldn't generate impls of std traits for std types in user code.
5. **Field declaration hint**: `Type name` in a struct → "write it `name: Type`" (reuse the param hint). Same for `public`/`private`/`static` → "som has no `public`, use `pub`" (or whatever applies).
6. **Method discoverability**: hint for `fn` inside `struct` → "methods go in `impl Person { … }`"; `this` → `self`; using `self` without a parameter → "add `self: &Person`"; `interface` → `trait`; `new X(...)`/`X(...)`/`X.new` → `X { … }` or `X::new`.
7. **Moving `self` in getters**: the "used after consumed" error on a method receiver should directly suggest `self: &Person`.
8. **`&&`/`||`/`++`/`+=`/`?:`/`/* */`/`'c'`** → "som writes this as …" hints.
9. **Unknown method**: suggest near names (`getNam` → `getName`, `length`/`size` → `len`, `add` → `push`) and list available methods like fields do.
10. Don't suggest edits to std files (`make it pub`).
11. `none` hint: suggest the optional type (`String?`) too, and drop the duplicate "cannot infer" error.
12. Drop the generic "expected ;" when a specific hint fires on the same token (`class`).
