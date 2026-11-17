# TODO

Found by writing programs blind (only `som --help` and compiler output), 2026-11-17.

## Compile-time evaluation

### Bugs

- [ ] **The could-be-const hint can crash the compiler on ordinary code (no `@`).** No memory limit while evaluating; an allocation failure aborts the compiler process.
  ```som
  fn cap() -> usize { let l: List<i64> = List::with_capacity(1000000000000); l.len }
  fn main() { println("{cap()}"); }
  ```
  → `memory allocation of 128000000000000 bytes failed` + Rust backtrace note. Same with `@cap()`.
  Fix: memory budget next to the step budget; allocation failure becomes a normal diagnostic.

- [ ] **The hint makes `som check` slow and memory-hungry on ordinary code.** Few steps, lots of memory, so the step budget never fires. 3-line file: 22 s, 4.3 GB peak (~64× overhead for a 64 MB string). 2^28 reached 6.3 GB before being killed. Would also hit the LSP.
  ```som
  fn bomb() -> usize { let mut s = "x"; let mut i = 0; while i < 26 { s = s + s; i += 1; } s.len }
  fn main() { println("{bomb()}"); }
  ```
  Fix: the hint gets a much smaller budget (steps + memory + time) than `@`, and gives up silently. Also look at why a string costs ~64× its size.

- [ ] **`@` on an impure function silently drops its side effects.** The print happens while compiling; the program no longer prints. Under `som run` the compiler's buffered output shows up after the program's output.
  ```som
  fn a() -> i64 { println("A"); 1 }
  fn main() { println("start"); let x = @a(); println("end {x}"); }
  ```
  The hint already knows `a` is impure (it doesn't fire for it). Make `@` on an impure call an error, or at least a warning.

- [ ] **Recursion-depth limit is reported as the step limit.** `down(10001)` fails, `down(9000)` works, yet the message says "it ran 10000000 steps without finishing: it may never end".
  ```som
  fn down(n: i64) -> i64 { if n == 0 { 0 } else { 1 + down(n - 1) } }
  fn main() { println("{@down(10001)}"); }
  ```
  Give the depth limit its own message. The same error also repeats the "`down(…)` is called here" label on the same span.

- [ ] **`i64::MIN / -1` gives `1` at run time.** Compile time correctly rejects it ("divides the smallest number by -1"), but the prover lets the run-time version through, and the result is neither a wrap nor a trap.
  ```som
  fn mindiv(a: i64, b: i64) -> i64 { if b != 0 { a / b } else { 0 } }
  // run-time call with a = MIN, b = -1 prints 1
  ```

### Rough edges

- [ ] Internal names leak into messages: "constant `@const` is made of itself", "needs `@const` to work out `@const`", "`@const.mem.ptr` / `@const.items.mem.ptr` is a reference to memory that only exists while compiling". Name the call (`@outer()`) and the type (`List`) instead.
- [ ] Big constants are baked in silently: a 32 MB string from `@text()` gives a 33.8 MB binary, even when only `.len` is used. Warn above some size.
- [ ] `inf`/NaN can't be a compile-time constant, but are fine at run time. Defensible, but the message should say why.

### Held up (keep these working)

Infinite loops (10M-step budget, points at the running line), constant cycles (`A = B + 1; B = A + 1`), cycles through `@` in functions, run-time values passed to `@`, divide by zero (rejected by the prover), generics + traits at compile time. Run-time and compile-time results agree for `1 << 64`, `300 as u8`, `-1 as usize`, `-MIN`, i32 overflow, `-7 % 2`, i64 wrap, usize underflow.

## Language / diagnostics (newcomer friction)

### Bugs

- [ ] **Calling a function value doesn't work.** `f: fn(i64) -> i64` is accepted as a parameter type, but `f(x)` says "cannot find function `f`"; `(f)(x)` and `f.call(x)` fail too. Meanwhile the no-closures error tells you to "declare one with a name and pass that".
- [ ] **Indexing a string literal contradicts indexing a string variable.** `s[1]` works on `let s = "abc"`, but `" "[0]` says "cannot index a string with `[]` … doesn't implement `Index`".
- [ ] **A `}` inside interpolation ends it.** `"{@total(Sq { s: 4 })}"` fails, and the suggested fix (`{ s: 4} }`) is wrong.
- [ ] **The suggested fix for a match guard is invalid.** `x if x == PLUS => …` suggests `x => if x == PLUS => …`, and causes 3 follow-on errors.

### Missing "som does it like this" hints

Rust users hit these first; give them the same treatment as `const` → `@const`, `Vec` → `List`, `or` → `||`:
- [ ] `out` (and other reserved words) as a name: "expected identifier, found `out`" should say it's a keyword. It made me think `let mut` didn't exist.
- [ ] Tuples (`(i64, i64)`, `let (a, b) = …`): plain parse errors. Suggest a struct.
- [ ] `Result` / `Ok` / `Err` / `?`: "cannot find type" or parse errors. Say what to use instead.
- [ ] Char/byte literals (`' '`, `b' '`): "expected an expression". Say how to write a byte.
- [ ] Match guards (`pat if cond`): no explanation that they don't exist.
- [ ] `Vec::new` doesn't get the `List` hint that `Vec<T>` gets.
- [ ] A string literal inside `"{…}"` ends the outer string; the error talks about braces instead of the quote. Suggest binding it to a variable.

### Diagnostic wording

- [ ] Bounds proof: "`t.len - 1` is only known to be at most `t.len - 1`" is a tautology. The suggested guard `if t.len - 1 < t.len` makes sense only to the prover; suggest `if t.len >= 2`.
- [ ] A `PLUS =>` pattern when `@const PLUS` is in scope: the unreachable-arm error is right, but should say that `PLUS` binds a new name and doesn't compare against the constant.
- [ ] Integer widths: `usize < i64` says "no such operation"; suggest `as`.
- [ ] Root cause last: "cannot infer the type of `Map::empty()`" was printed after the 8 errors it caused.
- [ ] Identical errors aren't collapsed (7× `const` → `@const`).
- [ ] The "expected a type" list says `str`; the type is `string` everywhere else.
- [ ] Bounds proof can't see that a string literal's length is known (`let s = "abc"; s[1]`).

### Ergonomics

- [ ] `Map.get` returns `&V?`, so a word count needs `*(counts.get(&w) else &0)`.
- [ ] `List::of([…])` of structs needs `Copy`.
- [ ] `print` has no newline (fine, `println` exists); ignoring a `bool` result (`acct.withdraw(30);`) gives no warning.
