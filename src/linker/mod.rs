use std::{fs::File, io::Write, path::PathBuf, process::Command};

use cranelift::object::ObjectProduct;

use crate::diagnostics::Diagnostic;

type Result<T> = std::result::Result<T, Diagnostic>;

pub struct Linker {
    pub output: String,
    pub libraries: Vec<String>,
    pub library_paths: Vec<String>,
    pub needs_libc: bool,
}

enum LinkerFlavor {
    Unix,     // gcc, clang, ld.lld, ld64.lld
    MsvcLink, // lld-link, link.exe
}

enum LinkerError {
    NoLinkerFound,
    FailedToLink { stderr: String },
    IoError(std::io::Error),
}

impl LinkerError {
    fn to_diagnostic(self) -> Diagnostic {
        match self {
            LinkerError::NoLinkerFound => Diagnostic::error("no linker found"),
            LinkerError::FailedToLink { stderr } => {
                let mut diag = Diagnostic::error("failed to link");
                for line in stderr.lines() {
                    if !line.is_empty() {
                        diag = diag.with_trace(line);
                    }
                }
                diag
            }
            LinkerError::IoError(e) => Diagnostic::error(format!("io error: {}", e)),
        }
    }
}

impl Linker {
    pub fn new(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            libraries: Vec::new(),
            library_paths: Vec::new(),
            needs_libc: false,
        }
    }

    pub fn with_libraries(mut self, libraries: Vec<String>, needs_libc: bool) -> Self {
        self.libraries = libraries;
        self.needs_libc = needs_libc;
        self
    }

    pub fn with_library_paths(mut self, paths: Vec<String>) -> Self {
        self.library_paths = paths;
        self
    }

    /// Link an ObjectProduct directly into an executable
    pub fn link_object(&self, product: ObjectProduct) -> Result<PathBuf> {
        // Emit object bytes
        let object_bytes = product
            .emit()
            .map_err(|e| Diagnostic::error(format!("failed to emit object: {}", e)))?;

        // Write to temporary .o file
        let obj_path = PathBuf::from(format!("{}.o", self.output));
        let mut file =
            File::create(&obj_path).map_err(|e| LinkerError::IoError(e).to_diagnostic())?;
        file.write_all(&object_bytes)
            .map_err(|e| LinkerError::IoError(e).to_diagnostic())?;

        // Link the object file
        let result = self.link_modules(vec![obj_path.clone()]);

        // Clean up temp file (ignore errors)
        let _ = std::fs::remove_file(&obj_path);

        result
    }

    pub fn link_modules(&self, modules: Vec<PathBuf>) -> Result<PathBuf> {
        let (linker_path, flavor) = find_linker()?;

        let mut cmd = Command::new(&linker_path);

        match flavor {
            LinkerFlavor::Unix => {
                // On macOS, clang handles SDK/platform automatically
                if cfg!(target_os = "macos") {
                    if self.needs_libc {
                        cmd.arg("-lSystem");
                    }
                } else if self.needs_libc {
                    // Linux: link libc
                    cmd.arg("-lc");
                }
                cmd.arg("-o").arg(&self.output).args(modules);

                // Add library search paths
                for path in &self.library_paths {
                    cmd.arg(format!("-L{}", path));
                }

                // Add library flags
                for lib in &self.libraries {
                    if is_library_file(lib) {
                        // Full path to library/object file
                        cmd.arg(lib);
                    } else {
                        // Library name: -l<name>
                        cmd.arg(format!("-l{}", lib));
                    }
                }
            }
            LinkerFlavor::MsvcLink => {
                cmd.arg(format!("/OUT:{}", self.output))
                    .arg("/ENTRY:main")
                    .arg("/SUBSYSTEM:CONSOLE")
                    .args(modules);

                // Add library search paths for MSVC
                for path in &self.library_paths {
                    cmd.arg(format!("/LIBPATH:{}", path));
                }

                // Add library flags for MSVC
                for lib in &self.libraries {
                    if is_library_file(lib) {
                        cmd.arg(lib);
                    } else {
                        cmd.arg(format!("{}.lib", lib));
                    }
                }
            }
        }

        let output = cmd
            .output()
            .map_err(|err| LinkerError::IoError(err).to_diagnostic())?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(LinkerError::FailedToLink { stderr }.to_diagnostic());
        }

        // Return absolute path so runner can find it
        let output_path = PathBuf::from(&self.output);
        let absolute_path = std::env::current_dir()
            .map(|dir| dir.join(&output_path))
            .unwrap_or(output_path);

        Ok(absolute_path)
    }
}

fn find_linker() -> Result<(String, LinkerFlavor)> {
    // Prefer system C compilers (they know library paths)
    for compiler in &["cc", "gcc", "clang", "zig cc"] {
        if std::process::Command::new(compiler)
            .arg("--version")
            .output()
            .is_ok()
        {
            return Ok((compiler.to_string(), LinkerFlavor::Unix));
        }
    }

    // On Windows, try MSVC linkers
    if cfg!(target_os = "windows") {
        // Try rust-lld first (bundled with Rust)
        if let Some(rust_lld) = find_rust_lld() {
            return Ok(rust_lld);
        }
        // Try system MSVC link.exe
        if std::process::Command::new("link.exe")
            .arg("/?")
            .output()
            .is_ok()
        {
            return Ok(("link.exe".to_string(), LinkerFlavor::MsvcLink));
        }
    }

    Err(LinkerError::NoLinkerFound
        .to_diagnostic()
        .with_hint(
            "install a C compiler (gcc, clang) or ensure Rust toolchain is properly installed",
        )
        .with_hint("on Windows: install Visual Studio Build Tools or MinGW")
        .with_hint("on Linux: install gcc (apt install gcc) or clang (apt install clang)")
        .with_hint("on macOS: install Xcode Command Line Tools (xcode-select --install)"))
}

fn find_rust_lld() -> Option<(String, LinkerFlavor)> {
    // Get rustc's sysroot
    let output = std::process::Command::new("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let sysroot = String::from_utf8(output.stdout).ok()?;
    let sysroot = sysroot.trim();
    let sysroot_path = PathBuf::from(sysroot);

    // Use platform-specific lld variant
    let (lld_name, flavor) = if cfg!(target_os = "windows") {
        ("lld-link.exe", LinkerFlavor::MsvcLink)
    } else if cfg!(target_os = "macos") {
        ("ld64.lld", LinkerFlavor::Unix)
    } else {
        ("ld.lld", LinkerFlavor::Unix)
    };

    // Get the target triple
    let target_output = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .ok()?;

    let target_info = String::from_utf8(target_output.stdout).ok()?;
    let target_triple = target_info
        .lines()
        .find(|line| line.starts_with("host: "))?
        .strip_prefix("host: ")?
        .trim();

    // Check common locations
    let candidates = vec![
        sysroot_path.join("bin").join(lld_name),
        sysroot_path
            .join("lib")
            .join("rustlib")
            .join(target_triple)
            .join("bin")
            .join(lld_name),
        sysroot_path
            .join("lib")
            .join("rustlib")
            .join(target_triple)
            .join("bin")
            .join("gcc-ld")
            .join(lld_name),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return Some((candidate.to_str()?.to_string(), flavor));
        }
    }

    None
}

/// Check if this is a direct library/object file path (vs a library name)
fn is_library_file(lib: &str) -> bool {
    lib.ends_with(".a")      // static library (Unix)
        || lib.ends_with(".o")    // object file (Unix)
        || lib.ends_with(".so")   // shared library (Linux)
        || lib.ends_with(".dylib") // dynamic library (macOS)
        || lib.ends_with(".lib")  // static/import library (Windows)
        || lib.ends_with(".obj")  // object file (Windows)
        || lib.ends_with(".dll") // dynamic library (Windows)
}
