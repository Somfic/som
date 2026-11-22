# Shaders

*Status (2026-11-22): `@fragment` shaders, and closures written as shaders, compile to GLSL
3.30 and run through raylib (`example/main.som`, `example/shader`, `example/shooter`), and to
WGSL and run through WebGPU (wgpu-native, `example/wgpu`), as do compute kernels. `@vertex`
is parsed but not compiled yet.*

A shader is ordinary som code that runs on the GPU for each pixel: a closure that takes a
`Fragment`, written where it's drawn, or a function marked `@fragment`. It's compiled for
the GPU while compiling, everything it calls with it, and it still runs (and can be tested)
on the CPU.

## A closure: what it captures are its uniforms

```som
let mut shader = RaylibShader::empty();
while !window_should_close() {
    let time = get_time() as f32;
    shader.fullscreen((frag: Fragment) => vec4(frag.uv.x, 0.5 + 0.5 * time.sin(), 0.0, 1.0));
}
```

The closure is the shader; `time`, which it captures, is a uniform named `time`, set from
each frame's closure. There's no struct of uniforms to write and nothing to set by hand: a
capture is a uniform by its name, and one that's a struct of the program's own is set field
by field (`camera.pos`). Captures have to copy, since each is a value for the GPU. A closure
captures the parts of a variable it reads when the variable itself doesn't copy, so
`g.camera` and `g.level.texture` inside it capture those two (as uniforms `camera` and
`g_level_texture`: the whole path where GLSL has the short name, or two would share it), not
all of `g`. A part that doesn't copy is still lent, and an error says what to capture
instead.

Each closure written out is its own shader (its own type), compiled once. raylib's adapter
loads it the first time it's drawn, which is why `RaylibShader::empty()` is written before
the loop and the closure inside it. `gpu::Shader::closure(&c)` makes the `gpu::Shader` of
one.

## A `@fragment` function and a struct of uniforms

```som
struct Uniforms { pub time: f32, pub tint: Vec4 }

@fragment
fn stripes(frag: Fragment, u: &Uniforms) -> Vec4 {
    let k = 0.5 + 0.5 * (frag.uv.x * 10.0 + u.time).sin();
    vec4(u.tint.rgb * k, 1.0)
}

let mut shader: RaylibShader<Uniforms> = gpu::Shader::fragment(stripes);
shader.fullscreen(Uniforms { time, tint });   // or shader.draw(rect, u)
```

`@glsl(stripes)` is its source as a constant. Every `@fragment` function is compiled for the
GPU, used or not, so `som check` reports a broken one.

## `gpu::Shader<U>`

A `Shader<U>` is a shader's GLSL (`glsl`, made while compiling: `glsl(f)` costs nothing when
it runs) and the type of its uniforms, `U`: a struct, or the closure. `shader.each(&u, (i,
name, value) => …)` calls back with each uniform's index, its name as the GPU has it (a
`cstr`), and its value as a `gpu::Uniform` (`Float`, `Int`, `Bool`, `Vec2`–`Vec4`, `Texture`,
or `Numbers` for an array or an `i32` vector). It's `gpu::each_uniform`, whose body the
compiler writes for each `U` (`crates/compiler/6-lower/src/uniforms.rs`): a struct's fields,
going into the program's own structs among them, or a closure's captures; each value made
with `Uniform::from(&field)`. No list, no loop. Nothing in it knows about a graphics library:
an adapter loads `glsl` and sets each uniform by name, from `each`.
`example/shader/raylib_shader.som` is raylib's (`impl From<gpu::Shader<U>> for
RaylibShader<U>`, or `RaylibShader::empty()` for a closure). It draws with
`shader.fullscreen(u)` (the whole screen, `frag.uv` from 0 to 1 across it) or
`shader.draw(rect, u)`, looks up each uniform's location by name as it sets it, and loads a
`gpu::Grid` with `load_grid`.

The uniforms' fields have to be `pub` for now: `each` reads them from the standard library.

## WebGPU: the same shaders in WGSL

`gpu::wgsl(shader)` is `gpu::glsl` in WGSL, for WebGPU. The same closures draw on either:

```som
let mut gpu = Gpu::open("wgpu", 800, 450);        // example/wgpu/webgpu.som
let mut shader = WgpuShader::empty();
while gpu.frame() {
    let time = gpu.time();
    shader.fullscreen(&mut gpu, (frag) => vec4(frag.uv.x, 0.5 + 0.5 * time.sin(), 0.0, 1.0));
    gpu.present();
}
```

`example/wgpu/webgpu.som` opens a window with SDL3 and draws with wgpu-native
(`brew install wgpu-native sdl3`), both imported from their headers. What WGSL makes different:

- **One uniform buffer.** WGSL has no loose uniforms, so every uniform but a texture is a member
  of `SomUniforms` (group 0, binding 0), in the order `each_uniform` gives them. The host packs
  them by WGSL's rules: each at the next multiple of its alignment (4 for a number, 8 for a
  `vec2`, 16 for a `vec3`/`vec4`). An array's items are 16 bytes apart, so `[f32; N]` and
  `[Vec2; N]` are declared as arrays of `vec4`s, and the shader copies out the first of each.
  A `bool` is a `u32`.
- **Textures and a sampler.** The sampler is binding 1, each texture the next, in order.
  `sample` is `textureSampleLevel`, which (unlike `textureSample`) can be inside a branch.
- **Its own vertex shader.** The module has `vs_main`, one triangle over the viewport, and
  `fs_main`, which calls the shader. The host sets the viewport to where the shader is drawn.
  `frag.position` is in pixels from the top left (GLSL's `gl_FragCoord` is from the bottom left).

`tests/tests/wgsl.rs` checks every compiling shader in `tests/corpus/shaders` with naga, wgpu's
own shader compiler. `SOM_WGSL_EXTRA=example/shooter` checks one more program's.

## Compute kernels

A kernel is a closure given an index, `(i) => …`, run on the GPU once for each item of the
buffer it fills, giving that item. What it captures are its uniforms, as a shader's are, and
a `gpu::Buffer<T>` among them is read with `get`:

```som
let input = gpu.upload(&numbers);                 // a gpu::Buffer<f32>
let output: gpu::Buffer<f32> = gpu.buffer(1000);
let mut double = Kernel::empty();
double.fill(&mut gpu, &output, (i) => input.get(i) * 2.0 + 1.0);
gpu.read(&output, &mut back);
```

`gpu::kernel(f)` is its WGSL: `cs_main`, 64 threads to a group, storing what the closure gives
at `i` for each `i` below the buffer's length (or a function marked `@compute`, taking the
index and a `&` to a struct of uniforms). A shader can capture a buffer too: `example/wgpu`
steps the Game of Life with a kernel and draws it with a shader, both reading the same buffer,
which never leaves the GPU.

A buffer's items are laid out as the CPU has them, so the host copies an array of them in and
out as it is. In WGSL a buffer is an `array<u32>`, and each item is read and written word by
word by a function made for it (`som_load_buf_cells`, `som_store_Particle`), converting with
`bitcast`. So an item is made of 4-byte numbers: `f32`, `i32`, `u32`, and vectors, arrays and
structs of them, which C lays out with no gaps. (Not `bool`, `usize` or enums.)

WGSL can't pass a pointer to a buffer to a function yet (naga doesn't take
`unrestricted_pointer_parameters`), so a buffer is read where it's captured, and its items
passed on.

`mem::size_of<T>()`, which the host needs to make a buffer, is the size the compile-time pass
lays `T` out at: as C does.

## Seeing the GLSL

`som build --emit wgsl` is the same in WGSL (`main.stripes.wgsl`).

`som build --emit glsl` writes every `@fragment` and `@vertex` function as the GPU gets it, a
file each next to the output (`main.stripes.frag`); `--emit glsl=DIR` puts them in DIR, and
`--emit glsl=-` prints them. Every shader is compiled for the GPU while checking, used or not,
so `som check` and the editor report a broken one before anything calls `@glsl` on it.

## Vectors

`Vec2`, `Vec3` and `Vec4` are `Vector<N, f32>`, made with `vec2(x, y)`, `vec3(xy, z)`,
`vec4(rgb, a)` and so on. They have `+ - * /` (with another vector or an `f32`, on either side of `*`), `-v`,
`.x .y .z .w`, `.r .g .b .a`, `.xy .xyz .rgb`, and `dot`, `length`, `normalized`, `mix`,
`clamp`, `abs`, `min`, `max`; `f32` has `sin`, `cos`, `pow`, `sqrt`, `fract`, `clamp`, `mix`,
`smoothstep` and more as methods. In a shader each is GLSL's own; on the CPU, ordinary code.

## Textures

A `gpu::Texture` field of the uniforms is a texture to sample: `u.image.sample(uv)` gives its
colour at `uv` (each from 0 to 1). It's GLSL's `uniform sampler2D image`, used where it's
named, since GLSL can't put a sampler in a struct it builds; helpers take a `&gpu::Texture`.
A texture can't be kept in a variable or put in another struct, and only the GPU samples
one: on the CPU, `sample` fails. Its value for the host is `Uniform::Texture`, with the
graphics library's `id`. raylib's adapter turns a raylib texture into one (`From`), and binds
it with `SetShaderValueTexture`; it has to be set after `begin_shader_mode`, which starts a
new batch, so the adapter's `shader.begin(&uniforms)` does both.

## Uniform structs and arrays

A uniform can be a struct of the program's own (not generic, not the standard library's) whose
fields are uniforms too: GLSL's `uniform Camera camera;`, set field by field (`camera.pos`).
The same `Camera` can be the game's own state, passed as it is. One holding a texture is
declared with it (`struct Material_uniform { sampler2D image; vec4 tint; }`) and made without
it for the shader to pass around, since GLSL can't make a struct holding a sampler; reading
the texture, there or in a helper given the struct, reads the uniform, `material.image`. A
struct the uniforms hold twice can't have its texture read, which is said where it's read.

A uniform can also be an array of numbers or `f32` vectors, `[f32; 16]` or `[Vec4; 8]`:
GLSL's `uniform vec4[8] monsters;`, set at once from a pointer (`Uniform::Numbers`, raylib's
`SetShaderValueV`), and looped over in the shader with `for i in 0..u.monsters.len`. Two to
four numbers are a vector, `[f32; 3]` a `vec3` and `[i32; 3]` an `ivec3`.

## Grids: the same data on both sides

`gpu::Grid` is a grid of numbers from 0 to 255, `Grid::of(w, h, &cells)`, kept on the CPU and
loaded onto the GPU as `grid.texture` (raylib's adapter: `load_grid(&mut grid)`, a pixel per
cell, the number its red). Both a `Grid` and a `Texture` are `gpu::Cells`, with `cell(x, y)`
(0 outside), so code written for any `M: gpu::Cells` runs on both:

```som
fn raycast<M: gpu::Cells>(map: &M, pos: Vec2, ray: Vec2) -> Hit { … map.cell(x, y) … }

raycast(&g.level, a, dir)          // the game: line of sight, against its Grid
raycast(&u.map, u.camera.pos, ray) // the shader: the same function, on the texture
```

On the GPU, a texture's `cell` is `texelFetch` (`gl::texelFetch`, `gl::textureSize`); on the
CPU, only a `Grid` has cells to read. `example/shooter` draws and collides with one level this
way.

## A fragment shader

- Takes a `Fragment` (`uv`, `color`, `position`), a `&` to a struct of uniforms, or both.
- Gives a `Vector<4, f32>`: the pixel's colour.
- Each field of the uniforms struct (or capture of the closure) is a GLSL `uniform` **with
  its name**, so the host sets it by that name. It's an `f32`, `i32`, `bool`, a `Vector` or
  array of 2 to 4 `f32`s or `i32`s, an array of numbers or `f32` vectors, a `Texture`, or a
  struct of the program's own holding those.
- Names match raylib's default shaders (`fragTexCoord`, `fragColor`, `finalColor`), so a
  som fragment shader works with raylib's own vertex shader.

## `@if GPU`: the GPU is a system

The GPU is treated like an OS in `@if WINDOWS { … }`. `som::gl` declares GLSL's built-in
functions (`extern "glsl" { … }`), which exist only on the GPU, as `som::c`'s exist only on
the OSes. Code picks with `@if GPU`:

```som
pub fn sin(rads: f32) -> f32 {
    @if GPU { return gl::sin(rads); } else { return c::sinf(rads); }
}
```

What can run where is worked out over the call graph by the same check as for OSes
(`driver/src/targets.rs`): a `@fragment` function has to run on the GPU, so one that reaches
`c::printf` is reported with the chain, `chatty → helper → println → c::printf`, and calling
`gl::floor` from `main` is reported as not available on `macos`.

Unlike an OS `@if`, `@if GPU` isn't decided while loading, because a shader is compiled for
both in one build: for the GPU by `@glsl`, and for the CPU as an ordinary function. Both
branches are resolved and type-checked (`Ast::gpu_ifs`); lowering makes a GPU copy of each
function a shader reaches (`Mir::gpu`), taking the GPU's branch there and the other branch in
the CPU's copy. C codegen leaves the GPU copies out. On the GPU, an OS `@if` takes the
target's branch.

`@if` is a statement, so a function written this way `return`s from both branches.

## What else can't be on the GPU

These are errors from the translator, reported where the shader reaches them and through
which calls:

- Text, pointers, slices, raw memory (`List`, `string`, `Rc`, …).
- 64-bit numbers and 8/16-bit integers. `usize` becomes `uint` (32 bits).
- Failing (`@fail`, `assert`), recursion, function values, and keeping a reference rather
  than passing it to a call.

## How it's compiled (`crates/compiler/shader`)

`@glsl` is the standard library's `som::gpu::glsl`, which the compile-time interpreter
recognises (`Mir::glsl`): lowering records which shader each instance is for, because a
function passed by name holds no value. The interpreter calls `som_shader::glsl(mir, entry)`
on the program's MIR and puts the source in the program as text.

The translation is MIR → GLSL, function by function:

| MIR | GLSL |
|---|---|
| struct / enum / optional / range | struct (an enum's tag is a `uint`) |
| `Vector<N, T>`, `[T; 2..4]` | GLSL's own `vecN`/`ivecN`/`bvecN`; std's `+ - * /`, `.x`/`.xy`…, `magnitude`, `normalized` are GLSL's operators, swizzles, `length` and `normalize` |
| `&T` param | `T` (a copy: nothing changes it during the call) |
| `&mut T` param | `inout T`: copy-in/copy-out, which equals passing a pointer because the borrow checker rules out aliasing |
| local reference | the place it refers to, written out; indexes it reads are copied where it's made |
| `if`, `while`, `loop`, `for`, `match` | `if`/`else` and `for (;;)` with `break`/`continue`, laid out by dominators (`structure.rs`, not tied to GLSL) |
| what can't be laid out that way (a `break` out of two loops) | `switch` on the next block, inside `for (;;)` |

`tests/tests/glsl.rs` runs every compiling shader in `tests/corpus/shaders` through `glslc`.

WGSL is the same translation with a different `Target` (`lib.rs`): its own types (`vec4<f32>`,
`array<T, N>`), `var` declarations, `select` for `?:`, a `ptr<function, T>` for a `&mut T`
parameter (given `&place`, read through `(*p)`), WGSL's names for GLSL's built-ins, and the
uniforms flattened into one buffer (`entry.rs`, `wgsl_uniforms`).

## Next

See TODO.md, "Shaders".
