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

`@fragment` and `@compute` → WGSL via `gpu::shader(f)`/`gpu::kernel(f)`, running through the `wgpu` library (`libs/wgpu`). GLSL and the raylib adapter were dropped 2026-11-23.

- [x] `@if GPU`: GPU as a system, `som::gl` built-ins, availability over the call graph, GPU copies in lowering (2026-11-20).
- [ ] A generic function without a written return type whose body ends in a per-instance `@for f in T` can't be called as a statement ("cannot infer the type"); write the return type.
- [ ] `@if` as a value (`@if GPU { gl::sin(x) } else { c::sinf(x) }` as a function's tail); std now writes `return` in both branches.
- [ ] `@if GPU || WINDOWS`: mixing the GPU and OSes in one condition isn't understood (it's treated as an ordinary `@if`).
- [x] Shader-friendly std (2026-11-21): `Vec2`–`Vec4`, `vec2`/`vec3`/`vec4` (also `vec4(rgb, a)`), `.r .g .b .a .rgb`, `dot`/`length`/`normalized`/`mix`/`clamp`/`abs`/`min`/`max` on `f32` vectors, trig as `f32` methods; all GLSL's own in shaders.
- [x] `0.5 * v`: a built-in type can implement an operator when the other side isn't built in (2026-11-21).
- [x] `-x` through a `Negate` trait; `Vector<N, f32>` has it (2026-11-21).
- [ ] Scalar `f32` methods in shaders (`x.clamp(…)`, `x.sin()`) are still calls to translated std functions, not WGSL's directly.
- [ ] `@vertex` shaders, and drawing a vertex+fragment pair (meshes).
- [x] Textures: `gpu::Texture` uniforms → `sampler2D`, `t.sample(uv)`, raylib adapter binds them (2026-11-21).
- [ ] Sampling on the CPU (a `Texture` holding pixels), so shaders using textures can be tested there.
- [ ] Building a `Texture` inside a shader reports "kept in a variable" rather than something more specific.
- [x] Structured control flow (`structure.rs`): `if`/`for (;;)`/`break`/`continue`; `switch` loop only as a fallback (2026-11-21).
- [ ] Breaking out of two loops at once still falls back to the `switch` loop: use a flag variable instead.
- [x] `gpu::Shader<U>` (GLSL + uniform names + allocation-free `each`), `@name(f)`, `glsl(f)` at run time, raylib adapter in `example/shader/raylib_shader.som` (2026-11-21).
- [ ] Uniform structs need `pub` fields, since `Shader::each` reads them from std. Decide whether `@for f in T` copies may read private fields (reflection privilege).
- [x] `u32` uniforms (2026-11-23).
- [ ] A shader without uniforms can't use `Shader::fragment` (its signature bound takes `&U`): add a form for `fn(Fragment) -> Vector<4, f32>`.
- [x] Native vectors: `[T; 2..4]` and `Vector` are `vecN`; std's vector ops are GLSL operators (2026-11-21).
- [x] Every shader is checked (and `--emit glsl` writes it) whether or not `@glsl` asks for it.
- [ ] A local named `shader` hides `gpu::shader` for `@shader(f)`; write `gpu::shader(f)`.
- [ ] Generated names are long (`som_data_Vector_4_f32`); shorten where unambiguous.
- [x] WGSL emitter (`gpu::wgsl`, `--emit wgsl`, checked by naga) and a wgpu-native + SDL3 host in `example/wgpu` (2026-11-22).
- [x] Compute kernels (`gpu::kernel`, `@compute`, `gpu::Buffer<T>` read with `get`, `mem::size_of<T>()`), and a `Kernel` in `example/wgpu` running the Game of Life (2026-11-22).
- [ ] Pass a `gpu::Buffer` to a function on the GPU: specialise the function per buffer, until naga takes `unrestricted_pointer_parameters`.
- [ ] Kernels writing more than their own item (scatter, atomics), or more than one buffer; 2D dispatch for images.
- [ ] A `gpu::Buffer` on the CPU (holding its items), so kernels using `get` can be tested there.
- [x] The wgpu host as a library, `libs/wgpu`, found by `use wgpu;` through library roots (`$SOM_PATH`, then `libs/` beside std), with input, frame time, alpha blending and a `Painter` (rects, circles, a 5x7 font of its own) (2026-11-23).
- [x] The shooter, `example/wgpu` and `example/life` on `wgpu` (2026-11-23).
- [ ] The shooter runs at ~30 fps on a high density screen (4x raylib's pixels): draw the halls at a lower resolution, or in one pass with the HUD.
- [ ] Each `Shader::draw` is its own render pass and submit: batch a frame's draws.
- [ ] The `wgpu` library's window is macOS's (a Metal layer); Windows and Linux surfaces.
- [ ] The font's `g`, `p`, `q`, `y` have no room below the line.
- [ ] Libraries: no versions or dependencies yet, just folders found by name.
- [x] `Matrix<C, R>` with the sizes in the type (`Matrix<K, R> * Matrix<C, K>`), `Mat4` transforms, matrices as uniforms (2026-11-23).
- [ ] Matrices on the GPU are std's loops, not WGSL's `matCxR` and its `*`: once `@wgsl("…")` says how a std function is written in WGSL, use the native ones.
- [x] `@vertex` with a struct of the program's own passed to the fragment shader (`gpu::Vertex<S>`), `wgpu::Mesh`/`Material` with a depth buffer, `example/spheres` lit by Cook-Torrance (2026-11-23).
- [ ] Vertex shaders written as closures (lowering marks a shader closure's stage `Fragment` unless it's a kernel).
- [ ] Instancing, more than one vertex buffer, `u16` indices, back-face culling, MSAA.
- [x] Early `return`s (in an `if`, in a loop) and `for` with `break` are laid out as `if`s and loops, not the jump table; `tests/wgsl.rs` fails on any corpus shader that falls back. The shooter went from ~30 to ~56 fps (2026-11-23).
- [ ] Shaders are translated from MIR before the optimizer runs: copies (`t20 = t19; ret = t20;`), constants left unfolded (`5u + 1u`, `1.0 / 3.1415927`), a wrapper per std math method (`f_som_f32_max` around `max`). The GPU's compiler cleans these up, but the WGSL is 2-3x longer than it needs to be.
- [x] Uniforms are read where they are (`som_u.glyphs[i].x`, a matrix column `som_u.model[i]`): the shader's `&U` isn't passed, and a function given a `&` into the uniforms gets a copy of its own reading them in place (`f_halls_u`). Only copying a whole struct of them into a variable still makes it (2026-11-23).
- [ ] The uniforms both stages of a `Material` take are all declared in each, used or not.
- [ ] `@builtin(position)` and `front_facing` for a fragment shader taking what's passed on.
- [ ] Imported C enums whose variants start with a digit keep their whole name (`WGPUTextureDimension::WGPUTextureDimension2D`).
- [ ] `static const` flags in headers (`WGPUBufferUsage_Uniform`) aren't imported; the wgpu host writes them out.
- [ ] Closures can't change what they capture, so the wgpu host packs uniforms through a raw pointer.

From writing `example/shooter` (a raycasting shooter, 2026-11-21), what made shaders harder than they should be:

- [x] A texture uniform before other uniforms shifted the later fields' indexes (translator panic); `Vec4` lost `.r` to a doc comment in std.
- [x] One level for both sides: `gpu::Grid` (CPU cells, loaded as a texture), `gpu::Cells` for `Grid` and `Texture`, so one generic `raycast` serves the game and the shader (2026-11-21).
- [x] Uniform arrays of `f32` vectors (`[Vec4; 8]`, `Uniform::Vectors`): the shooter draws every monster in its one pass (2026-11-21).
- [x] raylib adapter: `shader.fullscreen(&u)`, `shader.draw(rect, &u)`, `load_grid`/`unload_grid` (2026-11-21).
- [x] Uniforms nest the program's own structs (`camera: Camera`), set field by field through `@for f in @leaves(U)` (2026-11-21).
- [x] Uniform arrays of numbers (`[f32; 16]`, `[i32; 16]`) and `i32` vectors, `Uniform::Numbers`; `From<&[f32; N]>` picks with `@if N >= 2 && N <= 4` (2026-11-21).
- [x] A texture in a nested uniform struct: declared as `<Struct>_uniform` with its sampler, made without it, read as `material.image` (2026-11-21).
- [x] Shaders as closures: captures are the uniforms (`gpu::Shader::closure`, `RaylibShader::empty()` + `fullscreen(closure)`); `gpu::each_uniform`'s body is written by lowering for each struct or closure, names and all (2026-11-21).
- [ ] Arrays of structs as uniforms; a texture in a struct the uniforms hold twice.
- [x] A shader closure's parameter type is inferred, `(frag) =>`: `RaylibShader::empty()` is in an impl bounded by `U: fn(Fragment) -> Vector<4, f32>`, which gives `U`'s variable that signature (2026-11-22).
- [ ] Calling `each` with the old two-parameter callback says "can't match an unknown type" about the `match` inside, not that the callback takes three.
- [x] Capturing `g` (which doesn't copy) in a shader closure: the error names the parts it reads that copy, `let camera = g.camera;` (2026-11-22).
- [x] Disjoint captures, for every closure: one reading `g.camera` of a `g` that doesn't copy captures `g.camera` (the shortest part read that copies, else what's read, lent), so `g` can change while the closure lives. typeck works out the parts (`ClosureSig::captures`, `uses`); lowering reads each from its place (2026-11-22).
- [ ] `@leaves` decides "the program's own struct" by name while parsing; two structs with one name in different modules (neither the field's) aren't gone into.
- [ ] `Grid` cells are 0 to 255 (a pixel's red); wider values would need a float or integer texture.
- [ ] Printing a `Vector` (`"{v}"`) panics in lowering ("no implementation of trait method `display_len`") rather than saying it can't be shown.
- [ ] Captured variables as uniforms (`gpu::fragment((frag) => … g.pos …)`) would remove the uniforms structs altogether.
- [ ] `i32` has no `min`/`max`/`clamp`.

## Errors (2026-11-24, see docs/errors.md)

- [ ] What closures fail with: "anything" for now. Track a closure's set like a function's,
      and a generic function's over what its function parameters fail with.
- [ ] `fail Wrapper<i32> { .. }`: a `fail` arm for a generic error type. The `dyn` it's
      held in can tell them apart now; the checker's sets key kinds by struct only.
- [ ] Taking the value out of a failure in a `fail` arm; it's lent now.
- [ ] `e.is<T>()` on `Error`: method calls with type arguments don't parse.
- [ ] `Display` for an error type takes `display_len` and `display`: a shorthand for "shown as
      this string" would make error types one line.
- [ ] Missing `fail` arms for an enum nothing matches list every variant; a long enum makes
      a long message.

## Files (2026-11-25, see docs/errors.md "Files")

- [ ] `eprint`/`eprintln`, and stdin/stdout/stderr as `File`s.
- [ ] `remove`, `rename`, `list_dir`, `make_dir`.
- [ ] `lines()` stops at a read error as if the file ended; give `string!` items, or check
      `ferror` after.
- [ ] Closing in `Drop` ignores `fclose`'s result; `write` flushes, so only a failure at
      close itself is lost.
- [ ] Paths are bytes as given: on Windows `fopen` reads them in the ANSI code page, not
      UTF-8 (`_wfopen` needs a UTF-16 conversion).
- [ ] `read_all` copies byte by byte through a `List<u8>`: read straight into the string's
      memory.

## dyn (2026-11-24, see docs/dyn.md)

- [ ] `Rc::new(Five {})` where a `#dyn Size` is expected: the expected type doesn't reach a
      generic call's arguments, so `T` is `Five`. Written in two steps for now.
- [ ] `value as dyn Trait`, to make one where nothing expects it.
- [ ] Traits building on others with methods (`trait IndexMut: Index`), and with associated
      types (`dyn Iterator<Item = i32>`): not allowed yet.
- [ ] Methods taking `self` by value: could take the value off the heap; not allowed yet.
- [ ] Asking which type a `dyn` holds (`d.is<T>()`, a `match` on it): only `fail T` arms do.
- [ ] Calling straight to the method where only one type ever reaches a `dyn`.
- [ ] A `T` where a `dyn Trait?` is expected isn't made one.

Found along the way, not from this work:
- [ ] `example/lisp` doesn't compile: 8× "`share` is only called through `Share`".
- [ ] `som-cli` tests are stale: builds go to `.build/`, and clap words a bad `--emit` value
      itself (3 failures).
- [ ] `som_optimize` `functions_used_as_values_stay` and the LSP's
      `a_library_used_from_two_files_lists_its_functions_once` fail.
