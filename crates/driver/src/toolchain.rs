//! What the compiler assumes about the files and the system C toolchain around it.

use std::path::{Path, PathBuf};

pub const SOURCE_EXTENSION: &str = "som";

/// The file that makes a folder a program, and where its `main` code goes.
pub const MAIN_FILE: &str = "main.som";

/// The folder next to a program where imported C headers are cached.
pub const CACHE_DIR: &str = ".som";

pub const C_COMPILER_VAR: &str = "SOM_CC";
pub const DEFAULT_C_COMPILER: &str = "cc";

/// `$SOM_CC`, or `cc`.
pub fn c_compiler() -> String {
    std::env::var(C_COMPILER_VAR).unwrap_or_else(|_| DEFAULT_C_COMPILER.into())
}

/// som integers wrap on overflow, and som's extern declarations are the only ones that count.
pub const C_FLAGS: &[&str] = &["-std=c11", "-O1", "-w", "-fwrapv", "-fno-builtin"];

/// A static library (`libbox2d.a`) doesn't bring the libraries it uses, and libm,
/// separate from libc outside macOS, is the usual one.
pub fn c_link_flags() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &[]
    } else {
        &["-lm"]
    }
}

pub const LIBRARY_PREFIX: &str = "lib";

/// Every library file extension, on any system: what makes a library name a path.
pub const LIBRARY_EXTENSIONS: &[&str] = &["so", "dylib", "a", "dll", "lib"];

/// The library file extensions of this system, preferred first.
pub fn platform_library_extensions() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["dylib", "a"]
    } else if cfg!(windows) {
        &["dll", "lib"]
    } else {
        &["so", "a"]
    }
}

/// `raylib` of `libraylib.dylib`. `None` for a versioned file, `libgc.1.dylib`, which is
/// another copy of `libgc.dylib`.
pub fn library_name(file: &str) -> Option<&str> {
    let name = file.strip_prefix(LIBRARY_PREFIX)?;
    let name =
        (LIBRARY_EXTENSIONS.iter()).find_map(|ext| name.strip_suffix(ext)?.strip_suffix('.'))?;
    (!name.contains('.')).then_some(name)
}

/// The files a library name can be on this system: `raylib` is `libraylib.dylib` or
/// `libraylib.a`.
pub fn library_files(library: &str) -> Vec<String> {
    (platform_library_extensions().iter())
        .map(|ext| format!("{LIBRARY_PREFIX}{library}.{ext}"))
        .collect()
}

/// The system's library directories, in the order they're searched.
pub fn system_library_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(target_os = "macos") {
        dirs.push("/opt/homebrew/lib".into());
    }
    dirs.extend(["/usr/local/lib".into(), "/usr/lib".into()]);
    dirs
}

/// `root`, or the current directory for an empty path.
pub(crate) fn program_dir(root: &Path) -> &Path {
    if root.as_os_str().is_empty() {
        Path::new(".")
    } else {
        root
    }
}

/// Where generated files for the program in `root` are kept.
pub(crate) fn cache_dir(root: &Path) -> PathBuf {
    program_dir(root).join(CACHE_DIR)
}
