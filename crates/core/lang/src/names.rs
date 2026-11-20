pub const PRELUDE_MODULE: &str = "som";
pub const ENTRY_POINT: &str = "main";
pub const SELF_VALUE: &str = "self";
pub const SELF_TYPE: &str = "Self";
pub const FAIL_EVENT: &str = "fail";

pub const MEM_MODULE: &str = "som::mem";
/// Next to a program's `main.som`: what building it makes, executables and all.
pub const BUILD_DIR: &str = ".build";
/// In the build folder: the som source generated from C headers (`raylib.som`).
pub const IMPORTS_DIR: &str = "imports";
/// The C library, which `extern "c";` also `use`s.
pub const LIBC_MODULE: &str = "som::c";
/// GLSL's built-in functions: only on the GPU.
pub const GL_MODULE: &str = "som::gl";
/// The `extern "glsl"` block declaring them: not a library to link.
pub const GL_LIBRARY: &str = "glsl";
/// `som::GPU`: whether the code is compiled for the GPU, in `@if GPU { … }`.
pub const GPU_CONST: &str = "GPU";
pub const FORGET: &str = "forget";

pub const LEN: &str = "len";
pub const BYTES: &str = "bytes";
pub const RANGE_START: &str = "start";
pub const RANGE_END: &str = "end";

pub fn is_std_module(name: &str) -> bool {
    name == PRELUDE_MODULE
        || name
            .strip_prefix(PRELUDE_MODULE)
            .is_some_and(|rest| rest.starts_with("::"))
}
