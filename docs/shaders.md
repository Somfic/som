# Shaders

*Status (2026-11-20): `@fragment` shaders compile to GLSL 3.30 and run through raylib
(`example/shader`). `@vertex` is parsed but not compiled yet. WGSL/wgpu is planned.*

A shader is an ordinary som function with `@fragment` (or `@vertex`) on the line above it.
`@glsl(name)` compiles it, and everything it calls, for the GPU while compiling, giving its
source as a `string`. The function is still a normal function, so it runs and can be
tested on the CPU too.

```som
use som::gpu;

struct Uniforms { pub time: f32, pub tint: Vec4 }

@fragment
fn stripes(frag: Fragment, u: &Uniforms) -> Vec4 {
    let k = 0.5 + 0.5 * (frag.uv.x * 10.0 + u.time).sin();
    vec4(u.tint.rgb * k, 1.0)
}

fn main() {
    // …
    let shader: RaylibShader<Uniforms> = gpu::Shader::fragment(stripes);
    shader.set(&Uniforms { time, tint });
    begin_shader_mode(shader.raw);
    // …
}
```

## `gpu::Shader<U>`

`gpu::Shader::fragment(stripes)` makes a `Shader<U>` from the shader itself: `U` is worked
out from its signature, `glsl` is its source (made while compiling: `glsl(f)` costs nothing
when it runs, and `@glsl(f)` is the same as a constant), and `uniforms` are `U`'s field names
(`@name(f)`). `shader.each(&u, (i, value) => …)` calls back with each field's index and
value as a `gpu::Uniform` (`Float`, `Int`,
`Bool`, `Vec2`–`Vec4`), written out per field while compiling: no list, no loop. Nothing in it knows about a graphics library: an
adapter loads `glsl`, looks up each uniform by name once, and sets them from `each`.
`example/shader/raylib_shader.som` is raylib's, as `impl From<gpu::Shader<U>> for
RaylibShader<U>`, so a `gpu::Shader` becomes one wherever one is expected.

The uniforms' fields have to be `pub` for now: `each` reads them from the standard library.

## Seeing the GLSL

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

## A fragment shader

- Takes a `Fragment` (`uv`, `color`, `position`), a `&` to a struct of uniforms, or both.
- Gives a `Vector<4, f32>`: the pixel's colour.
- Each field of the uniforms struct is a GLSL `uniform` **with the field's name**, so the
  host sets it by that name. A field is an `f32`, `i32`, `u32`, `bool`, or a `Vector` of 2 to 4
  of those.
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

## Next

See TODO.md, "Shaders".
