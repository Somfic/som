//! What the compiler assumes about the files and the system C toolchain around it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use som_base::message as m;
use som_base::{Diagnostic, msg};

pub const SOURCE_EXTENSION: &str = "som";

/// The file that makes a folder a program, and where its `main` code goes.
pub const MAIN_FILE: &str = "main.som";

/// The folder next to a program where imported C headers are cached.
pub use som_lang::{BUILD_DIR, IMPORTS_DIR};

pub const C_COMPILER_VAR: &str = "SOM_CC";
pub const DEFAULT_C_COMPILER: &str = "cc";

/// `$SOM_CC`, or `cc`.
pub fn c_compiler() -> String {
    std::env::var(C_COMPILER_VAR).unwrap_or_else(|_| DEFAULT_C_COMPILER.into())
}

/// som integers wrap on overflow, and som's extern declarations are the only ones that count.
pub const C_FLAGS: &[&str] = &["-std=c11", "-O1", "-w", "-fwrapv", "-fno-builtin"];

pub const LIBRARY_PREFIX: &str = "lib";

/// The standard library's folder of C library declarations, `som::c`: one file per system.
pub const LIBC_BINDINGS_DIR: &str = "c";

/// The systems the C library's declarations are kept for, each with the target they're
/// generated on. Programs are checked against all of them.
pub const LIBC_TARGETS: &[(Os, Triple)] = &[
    (Os::Linux, Triple::X86_64Linux),
    (Os::MacOs, Triple::Aarch64MacOs),
    (Os::Windows, Triple::X86_64Windows),
];

/// Every library file extension, on any system: what makes a library name a path.
pub const LIBRARY_EXTENSIONS: &[&str] = &["so", "dylib", "a", "dll", "lib"];

/// `raylib` of `libraylib.dylib`. `None` for a versioned file, `libgc.1.dylib`, which is
/// another copy of `libgc.dylib`.
pub fn library_name(file: &str) -> Option<&str> {
    let name = file.strip_prefix(LIBRARY_PREFIX)?;
    let name =
        (LIBRARY_EXTENSIONS.iter()).find_map(|ext| name.strip_suffix(ext)?.strip_suffix('.'))?;
    (!name.contains('.')).then_some(name)
}

/// The C compiler, the system a program is compiled for, and the extra directories its
/// libraries are looked for in. Headers are read and programs linked with the same one,
/// so the imported declarations match the C library the program ends up with.
#[derive(Debug, Clone)]
pub struct Toolchain {
    /// A command, possibly with leading arguments: `zig cc`.
    pub cc: String,
    pub target: Target,
    /// Looked in before the system's directories (`-L`).
    pub library_dirs: Vec<PathBuf>,
}

impl Default for Toolchain {
    fn default() -> Self {
        Toolchain {
            cc: c_compiler(),
            target: Target::host(),
            library_dirs: Vec::new(),
        }
    }
}

impl Toolchain {
    /// The C compiler, set up for the target.
    pub fn c_command(&self) -> Command {
        let mut words = self.cc.split_whitespace();
        let mut cmd = Command::new(words.next().unwrap_or(DEFAULT_C_COMPILER));
        cmd.args(words);
        match (self.zig(), self.target.triple()) {
            (Some(_), Some(triple)) => {
                cmd.arg(format!("--target={}", triple.name()));
                cmd.args(self.target.sysroot_flag());
            }
            _ => {
                cmd.args(self.target.c_flags());
            }
        }
        cmd
    }
}

impl Toolchain {
    /// The C compiler, set up to read headers. `zig cc` can't: it expects an object file
    /// from every compile, and `-ast-dump` makes none. So headers are read by the clang
    /// in zig, given the include directories and defines `zig cc` would give it.
    pub fn header_command(&self) -> Command {
        let Some(zig) = self.zig() else {
            return self.c_command();
        };
        let mut cmd = Command::new(zig);
        cmd.arg("clang")
            .args(self.target.c_flags())
            .arg("-nostdinc")
            .args(self.zig_header_flags());
        cmd
    }

    /// The `zig` of `zig cc`.
    fn zig(&self) -> Option<&str> {
        let mut words = self.cc.split_whitespace();
        let program = words.next()?;
        let is_zig = Path::new(program)
            .file_stem()
            .is_some_and(|stem| stem == "zig");
        (is_zig && words.next() == Some("cc")).then_some(program)
    }

    /// The `-isystem` and `-D` flags of the clang command `zig cc -###` says it would run.
    fn zig_header_flags(&self) -> Vec<String> {
        // Zig still compiles it, leaving `null.o` wherever it runs.
        let output = self
            .c_command()
            .args(["-###", "-c", "-x", "c", NULL_DEVICE, "-o", NULL_DEVICE])
            .current_dir(std::env::temp_dir())
            .output();
        let Ok(output) = output else {
            return Vec::new();
        };
        let mut words = quoted_words(&String::from_utf8_lossy(&output.stderr)).into_iter();
        let mut flags = Vec::new();
        while let Some(word) = words.next() {
            if (word == "-isystem" || word == "-D")
                && let Some(value) = words.next()
            {
                flags.extend([word, value]);
            }
        }
        flags
    }

    /// Whether the C compiler has a C library for another target. Plain clang knows every
    /// target, but brings no headers or libraries for them, which would otherwise show
    /// up as `stdio.h` missing from the first header imported.
    pub fn check_target(&self) -> Result<(), NoCLibrary> {
        if !self.target.is_cross() {
            return Ok(());
        }
        let probe = self
            .c_command()
            .args(["-E", "-x", "c", "-o", NULL_DEVICE, "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(b"#include <stdio.h>\n")?;
                }
                child.wait_with_output()
            });
        match probe {
            // A compiler that can't run at all is reported when it's first needed.
            Ok(output) if !output.status.success() => Err(NoCLibrary {
                cc: self.cc.clone(),
                triple: self.target.triple().map_or("", Triple::name).to_string(),
                is_zig: self.zig().is_some(),
                reason: (String::from_utf8_lossy(&output.stderr).lines())
                    .find(|line| line.contains("error"))
                    .map(|line| line.trim().to_string()),
            }),
            _ => Ok(()),
        }
    }
}

const NULL_DEVICE: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

/// The `"…"` words of a command as `clang -###` prints it, unescaped.
fn quoted_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut word = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => word.extend(chars.next()),
                c => word.push(c),
            }
        }
        words.push(word);
    }
    words
}

#[derive(Debug, Clone)]
pub struct NoCLibrary {
    pub cc: String,
    pub triple: String,
    pub is_zig: bool,
    /// The compiler's own error.
    pub reason: Option<String>,
}

impl NoCLibrary {
    pub fn to_diagnostic(&self) -> Diagnostic {
        let mut d = if self.is_zig {
            Diagnostic::error(msg![
                m::command(&self.cc),
                " cannot compile for ",
                m::code(&self.triple)
            ])
        } else {
            Diagnostic::error(msg![
                m::command(&self.cc),
                " has no C library for ",
                m::code(&self.triple)
            ])
            .with_hint(msg![
                "use ",
                m::command("--cc \"zig cc\""),
                ", which brings one for every target, or point ",
                m::command("--sysroot"),
                " at one"
            ])
        };
        d.trace.extend(self.reason.clone());
        d
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Os {
    MacOs,
    Windows,
    /// Linux, and the BSDs, which look the same to the compiler: ELF, `.so`, a separate libm.
    Linux,
}

impl Os {
    pub const ALL: [Os; 3] = [Os::Linux, Os::MacOs, Os::Windows];

    /// As `som::target::Os` names it.
    pub fn variant(self) -> &'static str {
        match self {
            Os::Linux => "Linux",
            Os::MacOs => "MacOs",
            Os::Windows => "Windows",
        }
    }

    /// Its constant in `som`, true when compiling for it: `som::WINDOWS`.
    pub fn constant(self) -> &'static str {
        match self {
            Os::Linux => "LINUX",
            Os::MacOs => "MACOS",
            Os::Windows => "WINDOWS",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Os::MacOs => "macos",
            Os::Windows => "windows",
            Os::Linux => "linux",
        }
    }
}

/// A system som can compile for: 64-bit, with an OS the standard library has the C
/// library's declarations for. Linux's are glibc's, which musl mostly agrees with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Triple {
    X86_64Linux,
    Aarch64Linux,
    X86_64LinuxMusl,
    Aarch64LinuxMusl,
    X86_64Windows,
    Aarch64Windows,
    X86_64MacOs,
    Aarch64MacOs,
}

/// Each triple: its name, which zig understands too; as clang spells it; other common
/// spellings of it, Rust's among them; and its system.
const TRIPLES: &[(Triple, &str, &str, &[&str], Os)] = &[
    (
        Triple::X86_64Linux,
        "x86_64-linux-gnu",
        "x86_64-unknown-linux-gnu",
        &["x86_64-unknown-linux-gnu"],
        Os::Linux,
    ),
    (
        Triple::Aarch64Linux,
        "aarch64-linux-gnu",
        "aarch64-unknown-linux-gnu",
        &["aarch64-unknown-linux-gnu"],
        Os::Linux,
    ),
    (
        Triple::X86_64LinuxMusl,
        "x86_64-linux-musl",
        "x86_64-unknown-linux-musl",
        &["x86_64-unknown-linux-musl"],
        Os::Linux,
    ),
    (
        Triple::Aarch64LinuxMusl,
        "aarch64-linux-musl",
        "aarch64-unknown-linux-musl",
        &["aarch64-unknown-linux-musl"],
        Os::Linux,
    ),
    (
        Triple::X86_64Windows,
        "x86_64-windows-gnu",
        "x86_64-w64-windows-gnu",
        &["x86_64-pc-windows-gnu", "x86_64-w64-mingw32"],
        Os::Windows,
    ),
    (
        Triple::Aarch64Windows,
        "aarch64-windows-gnu",
        "aarch64-w64-windows-gnu",
        &["aarch64-pc-windows-gnullvm", "aarch64-w64-mingw32"],
        Os::Windows,
    ),
    (
        Triple::X86_64MacOs,
        "x86_64-macos",
        "x86_64-apple-macos",
        &["x86_64-apple-darwin"],
        Os::MacOs,
    ),
    (
        Triple::Aarch64MacOs,
        "aarch64-macos",
        "arm64-apple-macos",
        &["aarch64-apple-darwin", "arm64-apple-macos"],
        Os::MacOs,
    ),
];

impl Triple {
    pub const ALL: [Triple; 8] = [
        Triple::X86_64Linux,
        Triple::Aarch64Linux,
        Triple::X86_64LinuxMusl,
        Triple::Aarch64LinuxMusl,
        Triple::X86_64Windows,
        Triple::Aarch64Windows,
        Triple::X86_64MacOs,
        Triple::Aarch64MacOs,
    ];

    fn row(
        self,
    ) -> &'static (
        Triple,
        &'static str,
        &'static str,
        &'static [&'static str],
        Os,
    ) {
        TRIPLES
            .iter()
            .find(|row| row.0 == self)
            .expect("every triple has a row")
    }

    /// `x86_64-linux-gnu`.
    pub fn name(self) -> &'static str {
        self.row().1
    }

    /// As clang spells it: `x86_64-unknown-linux-gnu`.
    pub fn clang(self) -> &'static str {
        self.row().2
    }

    pub fn aliases(self) -> &'static [&'static str] {
        self.row().3
    }

    pub fn os(self) -> Os {
        self.row().4
    }

    /// The triple named `name`, or spelled that way.
    pub fn named(name: &str) -> Option<Triple> {
        (TRIPLES.iter())
            .find(|row| row.1 == name || row.3.contains(&name))
            .map(|row| row.0)
    }

    /// The system som runs on, if it's one it can compile for.
    pub fn host() -> Option<Triple> {
        let arm = cfg!(target_arch = "aarch64");
        let triple = match (cfg!(target_os = "macos"), cfg!(windows), arm) {
            (true, _, true) => Triple::Aarch64MacOs,
            (true, _, false) => Triple::X86_64MacOs,
            (_, true, true) => Triple::Aarch64Windows,
            (_, true, false) => Triple::X86_64Windows,
            (_, _, true) => Triple::Aarch64Linux,
            (_, _, false) => Triple::X86_64Linux,
        };
        (cfg!(target_pointer_width = "64")).then_some(triple)
    }
}

impl std::fmt::Display for Triple {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The system a program is compiled for: the one som runs on, or another named by a
/// target triple (`x86_64-linux-musl`), whose headers and libraries are in `sysroot`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    triple: Option<Triple>,
    sysroot: Option<PathBuf>,
}

impl Target {
    pub fn host() -> Target {
        Target::default()
    }

    /// The system som runs on, named, is just that: built into `.build/`, with its libraries.
    pub fn new(triple: Option<Triple>, sysroot: Option<PathBuf>) -> Target {
        let triple = triple.filter(|&t| Some(t) != Triple::host() || sysroot.is_some());
        Target { triple, sysroot }
    }

    pub fn triple(&self) -> Option<Triple> {
        self.triple
    }

    /// Whether this is another system than the one som runs on, whose libraries and
    /// headers aren't the ones installed here.
    pub fn is_cross(&self) -> bool {
        self.triple.is_some()
    }

    pub fn os(&self) -> Os {
        match self.triple {
            Some(triple) => triple.os(),
            None if cfg!(target_os = "macos") => Os::MacOs,
            None if cfg!(windows) => Os::Windows,
            None => Os::Linux,
        }
    }

    /// The width of a C `long`: 32 bits on Windows (LLP64) and 32-bit systems, 64 elsewhere.
    /// Every triple is 64-bit.
    pub fn long_bits(&self) -> u32 {
        let is_32 = self.triple.is_none() && cfg!(target_pointer_width = "32");
        if is_32 || self.os() == Os::Windows {
            32
        } else {
            64
        }
    }

    /// What tells clang which system to read headers and link for.
    pub fn c_flags(&self) -> Vec<String> {
        let target = self
            .triple
            .iter()
            .map(|t| format!("--target={}", t.clang()));
        target.chain(self.sysroot_flag()).collect()
    }

    fn sysroot_flag(&self) -> Option<String> {
        (self.sysroot.as_ref()).map(|s| format!("--sysroot={}", s.display()))
    }

    /// A static library (`libbox2d.a`) doesn't bring the libraries it uses, and libm,
    /// separate from libc on Linux, is the usual one.
    pub fn link_flags(&self) -> &'static [&'static str] {
        match self.os() {
            Os::Linux => &["-lm"],
            Os::MacOs | Os::Windows => &[],
        }
    }

    /// The library file extensions of the target, preferred first.
    pub fn library_extensions(&self) -> &'static [&'static str] {
        match self.os() {
            Os::MacOs => &["dylib", "a"],
            Os::Windows => &["dll", "lib", "a"],
            Os::Linux => &["so", "a"],
        }
    }

    /// The files a library name can be on the target: `raylib` is `libraylib.dylib` or
    /// `libraylib.a`.
    pub fn library_files(&self, library: &str) -> Vec<String> {
        (self.library_extensions().iter())
            .map(|ext| format!("{LIBRARY_PREFIX}{library}.{ext}"))
            .collect()
    }

    /// The system's library directories, in the order they're searched. Another system's
    /// are only in its sysroot: what's installed here is built for this one.
    pub fn system_library_dirs(&self) -> Vec<PathBuf> {
        if let Some(sysroot) = &self.sysroot {
            return ["usr/local/lib", "usr/lib", "lib"]
                .iter()
                .map(|dir| sysroot.join(dir))
                .collect();
        }
        if self.is_cross() {
            return Vec::new();
        }
        let mut dirs = Vec::new();
        if cfg!(target_os = "macos") {
            dirs.push("/opt/homebrew/lib".into());
        }
        dirs.extend(["/usr/local/lib".into(), "/usr/lib".into()]);
        dirs
    }

    /// The headers `extern "c";` imports from.
    pub fn libc_headers(&self) -> &'static [&'static str] {
        const WINDOWS: &[&str] = &[
            "stdio.h",
            "stdlib.h",
            "string.h",
            "math.h",
            "ctype.h",
            "time.h",
            "io.h",
            "process.h",
            "errno.h",
        ];
        const POSIX: &[&str] = &[
            "stdio.h",
            "stdlib.h",
            "string.h",
            "math.h",
            "ctype.h",
            "time.h",
            "unistd.h",
            "fcntl.h",
            "sys/wait.h",
            "errno.h",
        ];
        match self.os() {
            Os::Windows => WINDOWS,
            Os::MacOs | Os::Linux => POSIX,
        }
    }

    /// `program`, named as the target's executables are: `program.exe` on Windows.
    pub fn executable(&self, path: &Path) -> PathBuf {
        match self.os() {
            Os::Windows if path.extension().is_none() => path.with_extension("exe"),
            _ => path.to_path_buf(),
        }
    }

    /// The variable a program's shared libraries are found through when it runs.
    pub fn library_path_var(&self) -> &'static str {
        match self.os() {
            Os::MacOs => "DYLD_LIBRARY_PATH",
            Os::Windows => "PATH",
            Os::Linux => "LD_LIBRARY_PATH",
        }
    }
}

/// `root`, or the current directory for an empty path.
pub(crate) fn program_dir(root: &Path) -> &Path {
    if root.as_os_str().is_empty() {
        Path::new(".")
    } else {
        root
    }
}

/// Where building the program in `root` for `target` puts what it makes: `.build/`, and for
/// another system `.build/<triple>/`, so builds for each don't overwrite each other.
pub fn build_dir(root: &Path, target: &Target) -> PathBuf {
    let dir = program_dir(root).join(BUILD_DIR);
    match target.triple() {
        Some(triple) => dir.join(triple.name()),
        None => dir,
    }
}

/// Where the som source generated from C headers goes: `.build/imports/`, per target, since
/// each system's headers say different things.
pub(crate) fn imports_dir(root: &Path, target: &Target) -> PathBuf {
    build_dir(root, target).join(IMPORTS_DIR)
}
