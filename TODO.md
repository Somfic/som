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

## Parallelism (deferred, 2026-11-20)

Multicore CPU loops: a scoped fork-join `som::par` library on closures, made safe by the
borrow checker. Skipped for now in favour of GPU kernels; the pieces marked (shared) are
needed by the `@gpu` work too.

- [ ] Atomics: `load`/`store`/`fetch_add`/`compare_exchange` intrinsics, lowered to C11 `__atomic_*`.
- [ ] `std/thread`: spawn/join on pthreads, `CreateThread` under `@if WINDOWS`; core count via `sysconf`/`GetSystemInfo`.
- [ ] Pool + `par::for_each(items: &mut [T], f)` / `par::range`: chunks taken from one atomic counter, caller thread joins in, waits on a join counter. Scoped: returns only after every task ends, so lexical loans need nothing new.
- [ ] Each instance becomes a C `void chunk(void *env, size_t start, size_t end)` with the closure inlined, so clang can vectorize it.
- [ ] `@fail` on a worker: record the first, stop handing out chunks, raise it again on the caller after the join.
- [ ] Safety: a `Parallel` auto-trait (all fields `Parallel`; not `Rc`, whose count isn't atomic and is copied by `Share`; not `*T` unless opted in). One trait, not `Send`/`Sync`, while all work is scoped.
- [ ] Safety: a closure that must be `Parallel` may not write what it captures; suggest `par::sum` or writing into the element it's given.
- [ ] Safety (shared): functions unsafe on a worker (non-reentrant libc, future `static mut`) inferred over the call graph, reported with the chain, like `driver/src/targets.rs`.
- [ ] Reductions `par::sum`/`map_reduce` with fixed-size chunks combined in a fixed tree order, so results are bit-identical on any core count (replays, lockstep netcode).
- [ ] Port `example/raytracer` to render rows in parallel; later an n-body/particles example.
- [ ] Maybe later: `par for x in xs { }` sugar once the library is proven.

## Shaders (2026-11-20, see docs/shaders.md)

`@fragment` → GLSL 3.30 via `@glsl(f)`, running through raylib (`example/shader`).

- [x] `@if GPU`: GPU as a system, `som::gl` built-ins, availability over the call graph, GPU copies in lowering (2026-11-20).
- [ ] A generic function without a written return type whose body ends in a per-instance `@for f in T` can't be called as a statement ("cannot infer the type"); write the return type.
- [ ] `@if` as a value (`@if GPU { gl::sin(x) } else { c::sinf(x) }` as a function's tail); std now writes `return` in both branches.
- [ ] `@if GPU || WINDOWS`: mixing the GPU and OSes in one condition isn't understood (it's treated as an ordinary `@if`).
- [x] Shader-friendly std (2026-11-21): `Vec2`–`Vec4`, `vec2`/`vec3`/`vec4` (also `vec4(rgb, a)`), `.r .g .b .a .rgb`, `dot`/`length`/`normalized`/`mix`/`clamp`/`abs`/`min`/`max` on `f32` vectors, trig as `f32` methods; all GLSL's own in shaders.
- [x] `0.5 * v`: a built-in type can implement an operator when the other side isn't built in (2026-11-21).
- [x] `-x` through a `Negate` trait; `Vector<N, f32>` has it (2026-11-21).
- [ ] Scalar `f32` methods in shaders (`x.clamp(…)`, `x.sin()`) are still calls to translated std functions, not GLSL's directly.
- [ ] Externs can't be overloaded, so GLSL's two-argument `atan` is `gl::atan2`, renamed by the translator.
- [ ] `@vertex` shaders (inputs from raylib's `vertexPosition`/`vertexTexCoord`/`vertexColor`, `mvp`), and loading a vertex+fragment pair.
- [ ] Textures: a `Texture` uniform → `sampler2D`, `sample(tex, uv)`; raylib's `texture0`. Wants structured control flow first (implicit derivatives in non-uniform control flow are undefined).
- [x] Structured control flow (`structure.rs`): `if`/`for (;;)`/`break`/`continue`; `switch` loop only as a fallback (2026-11-21).
- [ ] Breaking out of two loops at once still falls back to the `switch` loop: use a flag variable instead.
- [x] `gpu::Shader<U>` (GLSL + uniform names + allocation-free `each`), `@name(f)`, `glsl(f)` at run time, raylib adapter in `example/shader/raylib_shader.som` (2026-11-21).
- [ ] Uniform structs need `pub` fields, since `Shader::each` reads them from std. Decide whether `@for f in T` copies may read private fields (reflection privilege).
- [ ] `u32` uniforms: dropped because raylib 5.x has no unsigned uniform type; back with WGSL/wgpu.
- [ ] A shader without uniforms can't use `Shader::fragment` (its signature bound takes `&U`): add a form for `fn(Fragment) -> Vector<4, f32>`.
- [ ] With `use som::gpu;`, `Shader` means std's, so raylib's has to be written `raylib::Shader`.
- [x] Native vectors: `[T; 2..4]` and `Vector` are `vecN`; std's vector ops are GLSL operators (2026-11-21).
- [x] Every shader is checked (and `--emit glsl` writes it) whether or not `@glsl` asks for it.
- [ ] `glsl` called without `@` fails at run time; make it a compile error.
- [ ] Generated names are long (`som_data_Vector_4_f32`); shorten where unambiguous.
- [ ] WGSL emitter + wgpu-native host, then compute kernels (`@compute`), see the Parallelism section.
