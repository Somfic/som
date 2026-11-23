# Shaders

*Status (2026-11-23): `@vertex` and `@fragment` shaders, closures written as shaders, and
compute kernels compile to WGSL and run through WebGPU, with the `wgpu` library (wgpu-native
and SDL3): `example/wgpu`, `example/life`, `example/shooter`, `example/spheres`. WGSL is the
only shader language.*

A shader is ordinary som code that runs on the GPU: for each corner of a mesh (a vertex
shader), each pixel (a fragment shader) or each item of a buffer (a compute kernel). It's a
closure written where it's used, or a function marked `@vertex`, `@fragment` or
`@compute`. It's compiled for the GPU while compiling,
everything it calls with it, and it still runs (and can be tested) on the CPU.

## What's where

- **The language** (`crates/compiler/shader`, lowering, the compile-time pass): `@fragment`,
  `@compute` and `@if GPU`; compiling a function and what it calls to WGSL; `each_uniform`'s
  body for each struct or closure; `mem::size_of<T>()`. It knows nothing of a window or a GPU
  API: what it makes is WGSL text and a way to walk the uniforms.
- **`som::gpu`** (`std/gpu`): what shaders are written with and what a host needs, on both
  sides. `Fragment`, `Texture`, `Buffer<T>`, `Grid`, `Cells`; `gpu::shader(f)` and
  `gpu::kernel(f)`, a shader's WGSL (worked out while compiling); `gpu::Shader<U>`, the WGSL
  and a `U`'s uniforms, given to a host as `gpu::Uniform`s.
- **`som::wgsl`** (`std/wgsl`): WGSL's built-in functions, `extern "wgsl"`, only on the GPU.
  Std's math and vectors are `@if GPU` on them.
- **`wgpu`** (`libs/wgpu`, not std): a host. A window and its input (SDL3) and the GPU
  (wgpu-native), both imported from their headers; `Shader` and `Kernel` load the WGSL and
  pack the uniforms; `Painter` draws flat shapes and text. It's a library that comes with
  som, found by `use wgpu;` (see below), and opinionated: std has no native libraries to
  need. Any other host could be written against `som::gpu` the same way.

## A closure: what it captures are its uniforms

```som
use wgpu;

let mut gpu = wgpu::Gpu::open("demo", 800, 450);
let mut background = wgpu::Shader::empty();
while gpu.frame() {
    let time = gpu.time();
    background.fullscreen(&mut gpu, (frag) => vec4(frag.uv.x, 0.5 + 0.5 * time.sin(), 0.0, 1.0));
    gpu.present();
}
```

The closure is the shader; `time`, which it captures, is a uniform, set from each frame's
closure. There's no struct of uniforms to write and nothing to set by hand. One that's a
struct of the program's own is set field by field (`camera.pos`). Captures have to copy,
since each is a value for the GPU. A closure captures the parts of a variable it reads when
the variable itself doesn't copy, so `g.camera` and `g.level.texture` inside it capture
those two, not all of `g`. A part that doesn't copy is still lent, and an error says what to
capture instead.

Each closure written out is its own shader (its own type), compiled once. `wgpu::Shader`
loads it the first time it's drawn, which is why `Shader::empty()` is written before the loop
and the closure inside it. `(frag) =>` needs no type: `empty()` is in an impl bounded by
`U: fn(Fragment) -> Vec4`. `shader.draw(&mut gpu, x, y, width, height, closure)` draws it over
part of the window instead (in pixels; `gpu.scale` is pixels to a point).

## A `@fragment` function and a struct of uniforms

```som
struct Stripes { pub time: f32, pub tint: Vec4, pub image: gpu::Texture }

@fragment
fn stripes(frag: Fragment, u: &Stripes) -> Vec4 {
    let k = 0.6 + 0.4 * ((frag.uv.x + frag.uv.y) * 10.0 + u.time).sin();
    vec4(u.image.sample(frag.uv).rgb * u.tint.rgb * k, 1.0)
}

let mut striped: wgpu::Shader<Stripes> = wgpu::Shader::fragment(stripes);
striped.fullscreen(&mut gpu, Stripes { time, tint, image });
```

Every `@fragment` function is compiled for the GPU, used or not, so `som check` reports a
broken one. A fragment shader takes a `Fragment` (`uv`, from (0, 0) at the top left to (1, 1)
at the bottom right of what's drawn; `color`; `position`, in pixels), a `&` to a struct of
uniforms, or both, and gives the pixel's colour, which is blended over what's there by its
alpha.

## `gpu::Shader<U>` and the uniforms

A `gpu::Shader<U>` is a shader's WGSL (`wgsl`, made while compiling: `gpu::shader(f)` costs
nothing when it runs) and the type of its uniforms, `U`: a struct, or the closure.
`shader.each(&u, (i, name, value) => …)` calls back with each uniform's index, its name and
its value as a `gpu::Uniform` (`Float`, `Int`, `Uint`, `Bool`, `Vec2`–`Vec4`, `Texture`,
`Buffer`, or `Numbers` for an array or an integer vector). It's `gpu::each_uniform`, whose
body the compiler writes for each `U` (`crates/compiler/6-lower/src/uniforms.rs`): a
struct's fields, going into the program's own structs among them, or a closure's captures;
each value made with `Uniform::from(&field)`. No list, no loop.

A uniform is an `f32`, `i32`, `u32`, `bool`, a vector of 2 to 4 numbers, an array of numbers
or `f32` vectors, a `Texture`, a `Buffer`, or a struct of the program's own holding those. The
fields have to be `pub` for now: `each` reads them from the standard library.

In WGSL every uniform but a texture or buffer is a member of one struct, `SomUniforms` (group
0, binding 0), in the order `each` gives them. The host packs them by WGSL's rules: each at
the next multiple of its alignment (4 for a number, 8 for a `vec2`, 16 for a `vec3`/`vec4`).
An array's items are 16 bytes apart, so `[f32; N]` and `[Vec2; N]` are declared as arrays of
`vec4`s, and the shader copies out the first of each. A `bool` is a `u32`. The sampler is
binding 1, and each texture and buffer the next, in order. The module has its own vertex
shader, `vs_main`, one triangle over the viewport, and `fs_main`, which calls the shader;
the host sets the viewport to where it's drawn.

The shader reads its uniforms where they are: its `&U` isn't passed, and `u.lights[i]` is
`som_u.lights[i].xyz`. A function given a `&` into the uniforms (`halls(frag, &s)`,
`ripple(&u.image, …)`) gets a copy of its own reading them in place too, `f_halls_u`: WGSL
can't pass a pointer to the uniform buffer, and copying it into each call would put every
array among them in the function's own memory for each pixel.

## Meshes: a `@vertex` shader and what it passes on

A vertex shader runs for each corner of a mesh. It takes a struct of the corner's numbers
and may take a `&` to a struct of uniforms; it gives a `gpu::Vertex<S>`: where the corner
lands, in clip space, and `passed`, an `S` of the program's own. The fragment shader drawn
with it takes that `S`, each field blended between the triangle's corners for the pixel:

```som
struct Corner { pub pos: Vec3, pub normal: Vec3 }
struct Surface { pub world: Vec3, pub normal: Vec3 }

@vertex
fn place(c: Corner, s: &Scene) -> gpu::Vertex<Surface> {
    let world = s.model * vec4(c.pos, 1.0);
    gpu::Vertex { position: s.view_projection * world, passed: Surface { world: world.xyz, normal: … } }
}

@fragment
fn shade(surface: Surface, s: &Scene) -> Vec4 { … }

let ball: wgpu::Mesh<Corner> = gpu.mesh(corners.as_slice(), indices.as_slice());
let mut material = wgpu::Material::new(place, shade);
material.draw(&mut gpu, &ball, Scene { … });
```

`Material::new` is bounded so that the `S` the vertex shader gives is the one the fragment
shader takes: a mismatch is a type error. Both take the same uniforms.

- **The corner** is a struct of numbers and vectors of them (`f32`, `i32`, `u32`, `Vec2`…
  `Vec4`): each field is a vertex attribute, `@location(i)` in field order, read from the
  mesh's buffer as the CPU lays the struct out. The host finds each field's format by going
  through the struct with `@for f in T` and a trait, `wgpu::Attribute`.
- **What's passed on** is the same: numbers and vectors. Each field is `@location(i)` of
  `SomVaryings`, declared alike in both modules, since both are made from `S`. Integers are
  taken from the first corner (`@interpolate(flat)`) rather than blended.
- **The two modules** are compiled apart, `gpu::shader(place)` and `gpu::shader(shade)`: a
  vertex module with `vs_main`, and a fragment module with `fs_main` taking `SomVaryings`. A
  fragment shader taking a `Fragment` instead keeps its own vertex shader, over a rectangle.
- **Depth**: `wgpu` keeps a depth buffer the size of the window, cleared by the first
  `Material` drawn each frame, so nearer pixels hide those behind.

`example/spheres` lights a grid of spheres by Cook-Torrance (GGX, Smith, Schlick), from
smooth to rough, metal and not, all in som.

## Compute kernels

A kernel is a closure given an index, `(i) => …`, run on the GPU once for each item of the
buffer it fills, giving that item. What it captures are its uniforms, as a shader's are, and
a `gpu::Buffer<T>` among them is read with `get`:

```som
let input = gpu.upload(&numbers);                 // a gpu::Buffer<f32>
let output: gpu::Buffer<f32> = gpu.buffer(1000);
let mut double = wgpu::Kernel::empty();
double.fill(&mut gpu, &output, (i) => input.get(i) * 2.0 + 1.0);
gpu.read(&output, &mut back);
```

`gpu::kernel(f)` is its WGSL: `cs_main`, 64 threads to a group, storing what the closure gives
at `i` for each `i` below the buffer's length (or a function marked `@compute`, taking the
index and a `&` to a struct of uniforms). A shader can capture a buffer too: `example/life`
steps the Game of Life with kernels and draws it with a shader, all reading buffers that
never leave the GPU.

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

## The `wgpu` library

`use wgpu;` finds it in `libs/`, beside the standard library: after a program's own folders,
`use name;` looks in each folder of `$SOM_PATH` (separated by `:`), then in `libs/`. A
library is a folder of `.som` files, as a program's own module is.

`wgpu::Gpu::open(title, width, height)` opens a window (`brew install wgpu-native sdl3`) and
the GPU. Each frame is `gpu.frame()` (false once it's closed, or Escape is pressed), drawing,
then `gpu.present()`. Also:

- Input: `gpu.pressed(wgpu::Key::R)` (since the frame before), `gpu.held(Key::W)`,
  `gpu.mouse()` (as a `uv`), `gpu.mouse_moved()` (after `gpu.grab_mouse()`), `left_held`,
  `right_held`.
- Time: `gpu.time()`, `gpu.frame_time()`, `gpu.fps()`.
- Data: `gpu.load_texture(w, h, pixels)`, `gpu.load_grid(&mut grid)`, `gpu.buffer<T>(len)`,
  `gpu.upload(&items)`, `gpu.write(&buffer, &items)`, `gpu.read(&buffer, &mut into)`.
- `wgpu::Painter::new(&mut gpu)`: `rect`, `circle`, `text` (a 5 by 7 font of its own, space to
  `~`) and `text_width`, in points. It's a `@fragment` function in `libs/wgpu` like any other.

## Seeing the WGSL

`som build --emit wgsl` writes every `@fragment` and `@compute` function as the GPU gets it, a
file each next to the output (`main.stripes.wgsl`); `--emit wgsl=DIR` puts them in DIR, and
`--emit wgsl=-` prints them. Every shader is compiled for the GPU while checking, used or not,
so `som check` and the editor report a broken one before anything asks for it.

`tests/tests/wgsl.rs` checks every compiling shader in `tests/corpus/shaders` with naga, wgpu's
own shader compiler. `SOM_WGSL_EXTRA=example/shooter` checks one more program's.

## Vectors

`Vec2`, `Vec3` and `Vec4` are `Vector<N, f32>`, made with `vec2(x, y)`, `vec3(xy, z)`,
`vec4(rgb, a)` and so on. They have `+ - * /` (with another vector or an `f32`, on either side
of `*`), `-v`, `.x .y .z .w`, `.r .g .b .a`, `.xy .xyz .rgb`, and `dot`, `length`,
`normalized`, `mix`, `clamp`, `abs`, `min`, `max`; `f32` has `sin`, `cos`, `pow`, `sqrt`,
`fract`, `clamp`, `mix`, `smoothstep` and more as methods. In a shader each is WGSL's own; on
the CPU, ordinary code.

## Matrices

`Matrix<C, R>` (`som::data`) is `C` columns of `R` rows of `f32`s, as WGSL's `matCxR<f32>`:
`Mat2`, `Mat3`, `Mat4`, made with `mat4(x, y, z, w)` from its columns, `Matrix::zero()` or
`Matrix::identity()`. A matrix times a vector of `C` numbers is a vector of `R`; a
`Matrix<K, R>` times a `Matrix<C, K>` is a `Matrix<C, R>`. The sizes are in the type, so
multiplying ones that don't fit is a type error, saying what does:

```
error: cannot apply `*` to `Matrix<2, 4>` and `Matrix<4, 4>`
     ┆ `Matrix<2, 4>` has `*` with `Vector<2, f32>`, `Matrix<C, 2>` or `f32` on the right
```

Also `+`, `-`, `* f32`, `transposed` (a `Matrix<R, C>`), `at(column, row)`, and for `Mat4`
the transforms: `translation`, `scaling`, `rotation_x/y/z`, `perspective` (depth 0 to 1, as
WebGPU has it) and `look_at`. Points are `vec4(p, 1.0)` and directions `vec4(d, 0.0)`.

As a uniform, a matrix is `Uniform::Matrix`: its columns, each 16 bytes after the one before
(an `array<vec4<f32>, C>`, each column cut to its `R` numbers in the shader). On the GPU its
operations are std's own code, translated like any other.

## Textures

A `gpu::Texture` among the uniforms is a texture to sample: `u.image.sample(uv)` gives its
colour at `uv` (each from 0 to 1). It's bound on its own, used where it's named, since WGSL
can't put a texture in a struct; helpers take a `&gpu::Texture`. A struct of the program's own
among the uniforms can hold one, and reading it there reads the binding. A texture can't be
kept in a variable or put in another struct, and only the GPU samples one: on the CPU,
`sample` fails. A struct the uniforms hold twice can't have its texture read, which is said
where it's read.

## Grids: the same data on both sides

`gpu::Grid` is a grid of numbers from 0 to 255, `Grid::of(w, h, &cells)`, kept on the CPU and
loaded onto the GPU as `grid.texture` (`gpu.load_grid(&mut grid)`, a pixel per cell, the
number its red). Both a `Grid` and a `Texture` are `gpu::Cells`, with `cell(x, y)` (0
outside), so code written for any `M: gpu::Cells` runs on both:

```som
fn raycast<M: gpu::Cells>(map: &M, pos: Vec2, ray: Vec2) -> Hit { … map.cell(x, y) … }

raycast(&g.level, a, dir)          // the game: line of sight, against its Grid
raycast(&u.map, u.camera.pos, ray) // the shader: the same function, on the texture
```

On the GPU, a texture's `cell` is `textureLoad`; on the CPU, only a `Grid` has cells to read.
`example/shooter` draws and collides with one level this way.

## `@if GPU`: the GPU is a system

The GPU is treated like an OS in `@if WINDOWS { … }`. `som::wgsl` declares WGSL's built-in
functions (`extern "wgsl" { … }`), which exist only on the GPU, as `som::c`'s exist only on
the OSes. Code picks with `@if GPU`:

```som
pub fn sin(rads: f32) -> f32 {
    @if GPU { return wgsl::sin(rads); } else { return c::sinf(rads); }
}
```

What can run where is worked out over the call graph by the same check as for OSes
(`driver/src/targets.rs`): a `@fragment` function has to run on the GPU, so one that reaches
`c::printf` is reported with the chain, `chatty → helper → println → c::printf`, and calling
`wgsl::floor` from `main` is reported as not available on `macos`.

Unlike an OS `@if`, `@if GPU` isn't decided while loading, because a shader is compiled for
both in one build: for the GPU by `gpu::shader`, and for the CPU as an ordinary function.
Both branches are resolved and type-checked (`Ast::gpu_ifs`); lowering makes a GPU copy of
each function a shader reaches (`Mir::gpu`), taking the GPU's branch there and the other
branch in the CPU's copy. C codegen leaves the GPU copies out. On the GPU, an OS `@if` takes
the target's branch.

`@if` is a statement, so a function written this way `return`s from both branches.

## What else can't be on the GPU

These are errors from the translator, reported where the shader reaches them and through
which calls:

- Text, pointers, slices, raw memory (`List`, `string`, `Rc`, …).
- 64-bit numbers and 8/16-bit integers. `usize` becomes `u32`.
- Failing (`@fail`, `assert`), recursion, function values, and keeping a reference rather
  than passing it to a call.

## How it's compiled (`crates/compiler/shader`)

`gpu::shader` and `gpu::kernel` are standard library functions the compile-time interpreter
recognises (`Mir::wgsl`): lowering records which shader each instance is for, because a
function passed by name holds no value. The driver compiles each with
`som_shader::wgsl(mir, entry)` and gives each instance a body returning the source.

The translation is MIR → WGSL, function by function:

| MIR | WGSL |
|---|---|
| struct / enum / optional / range | struct (an enum's tag is a `u32`) |
| `Vector<N, T>`, `[T; 2..4]` | WGSL's own `vecN<T>`; std's `+ - * /`, `.x`/`.xy`…, `magnitude`, `normalized` are WGSL's operators, swizzles, `length` and `normalize` |
| `[T; N]` | `array<T, N>` |
| `&T` param | `T` (a copy: nothing changes it during the call) |
| `&mut T` param | `ptr<function, T>`, given `&place` and read through `(*p)` |
| local reference | the place it refers to, written out; indexes it reads are copied where it's made |
| `if`, `while`, `loop`, `for`, `match` | `if`/`else` and `for (;;)` with `break`/`continue`, laid out by dominators (`structure.rs`) |
| what can't be laid out that way (a `break` out of two loops) | `switch` on the next block, inside `for (;;)` |

## Not yet

- A vertex shader written as a closure; a vertex shader without a fragment shader.
- Instancing, index buffers of `u16`s, more than one vertex buffer, culling.
- Pixel position and the facing in a fragment shader taking what's passed on.

## Next

See TODO.md, "Shaders".
