# Shaders

*Status (2026-11-20): `@fragment` shaders compile to GLSL 3.30 and run through raylib
(`example/shader`). `@vertex` is parsed but not compiled yet. WGSL/wgpu is planned.*

A shader is an ordinary som function with `@fragment` (or `@vertex`) on the line above it.
`@glsl(name)` compiles it, and everything it calls, for the GPU while compiling, giving its
source as a `string`. The function is still a normal function, so it runs and can be
tested on the CPU too.

```som
use extern "raylib";

struct Uniforms { time: f32, tint: Vector<4, f32> }

@fragment
fn stripes(frag: Fragment, u: &Uniforms) -> Vector<4, f32> {
    let k = 0.5 + 0.5 * f32::sin(frag.uv.x * 10.0 + u.time);
    let c = u.tint * k;
    Vector::new([c.x, c.y, c.z, 1.0])
}

fn main() {
    // …
    let shader = load_shader_from_memory(null() as cstr, @glsl(stripes).c_str);
    let time_at = get_shader_location(shader, "time");
    // set_shader_value(shader, time_at, &time as *, ShaderUniformDataType::Float as i32);
}
```

## A fragment shader

- Takes a `Fragment` (`uv`, `color`, `position`), a `&` to a struct of uniforms, or both.
- Gives a `Vector<4, f32>`: the pixel's colour.
- Each field of the uniforms struct is a GLSL `uniform` **with the field's name**, so the
  host sets it by that name. A field is an `f32`, `i32`, `u32`, `bool`, or a `Vector` of 2 to 4
  of those.
- Names match raylib's default shaders (`fragTexCoord`, `fragColor`, `finalColor`), so a
  som fragment shader works with raylib's own vertex shader.

## What can't be on the GPU

These are errors, reported where the shader reaches them and through which calls:

- Text, pointers, slices, raw memory (`List`, `string`, `Rc`, …).
- 64-bit numbers and 8/16-bit integers. `usize` becomes `uint` (32 bits).
- Calling C, except the math functions `f32`'s methods use (`sinf`, `sqrtf`, …), which map to
  GLSL's own functions.
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
| `Vector<N, T>` | a struct holding `T[N]`; converted to `vecN` only at the shader's inputs and output |
| `&T` param | `T` (a copy: nothing changes it during the call) |
| `&mut T` param | `inout T`: copy-in/copy-out, which equals passing a pointer because the borrow checker rules out aliasing |
| local reference | the place it refers to, written out; indexes it reads are copied where it's made |
| one block, or a chain of `goto`s | straight-line statements |
| any other control flow | `switch` on the next block, inside `for (;;)` |

`tests/tests/glsl.rs` runs every compiling shader in `tests/corpus/shaders` through `glslc`.

## Next

See TODO.md, "Shaders".
