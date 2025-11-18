use crate::{LinkerError, Result};
use std::{path::PathBuf, process::Command};

pub struct Linker {
    pub output: String,
}

enum LinkerFlavor {
    Unix,     // gcc, clang, ld.lld, ld64.lld
    MsvcLink, // lld-link, link.exe
}

impl Linker {
    pub fn new(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
        }
    }

    pub fn link_modules(&self, modules: Vec<PathBuf>) -> Result<PathBuf> {
        let (linker_path, flavor) = find_linker()?;

        let mut cmd = Command::new(&linker_path);

        match flavor {
            LinkerFlavor::Unix => {
                cmd.arg("-o").arg(&self.output).args(modules);
            }
            LinkerFlavor::MsvcLink => {
                cmd.arg(format!("/OUT:{}", self.output))
                    .arg("/ENTRY:main")
                    .arg("/SUBSYSTEM:CONSOLE")
                    .args(modules);
            }
        }

        let status = cmd
            .status()
            .map_err(|err| LinkerError::IoError(err).to_diagnostic())?;

        if !status.success() {
            return Err(LinkerError::FailedToLink.to_diagnostic());
        }

        Ok(PathBuf::from(self.output.clone()))
    }
}

fn find_linker() -> Result<(String, LinkerFlavor)> {
    // First, try to find rust-lld (bundled with Rust toolchain)
    if let Some(rust_lld) = find_rust_lld() {
        return Ok(rust_lld);
    }

    // Fall back to system linkers
    for compiler in &["cc", "gcc", "clang", "zig cc"] {
        if std::process::Command::new(compiler)
            .arg("--version")
            .output()
            .is_ok()
        {
            return Ok((compiler.to_string(), LinkerFlavor::Unix));
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
