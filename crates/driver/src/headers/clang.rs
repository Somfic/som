use serde_json::Value;
use std::path::Path;
use std::process::Output;

use super::include_dirs;
use crate::toolchain::Toolchain;

/// The header's syntax tree, as clang's JSON.
pub(super) fn syntax_tree(
    header: &Path,
    root: &Path,
    toolchain: &Toolchain,
) -> Result<Value, String> {
    let (compiler, output) = run(
        &["-Xclang", "-ast-dump=json", "-fsyntax-only", "-x", "c"],
        header,
        root,
        toolchain,
    );
    let output = output.map_err(|e| format!("could not run `{compiler}`: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first = stderr
            .lines()
            .find(|l| l.contains("error"))
            .unwrap_or("it reported errors");
        return Err(format!("clang could not read it: {}", first.trim()));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("could not read clang's output: {e}"))
}

/// The header after the preprocessor, keeping its `#define`s (`-E -dD`), which say what
/// each name stands for, and lines saying which file each is in.
pub(super) fn preprocessed(header: &Path, root: &Path, toolchain: &Toolchain) -> Option<String> {
    let (_, output) = run(&["-E", "-dD", "-x", "c"], header, root, toolchain);
    let output = output.ok()?;
    (output.status.success()).then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs the C compiler on `header`. The header's own `#include <…>`s are looked for
/// where the header was: `gc.h` includes `<gc/gc.h>`.
fn run(
    args: &[&str],
    header: &Path,
    root: &Path,
    toolchain: &Toolchain,
) -> (String, std::io::Result<Output>) {
    let output = toolchain
        .header_command()
        .args(args)
        .args(
            include_dirs(root, toolchain)
                .iter()
                .filter(|dir| dir.is_dir())
                .map(|dir| format!("-I{}", dir.display())),
        )
        .arg(header)
        .output();
    (toolchain.cc.clone(), output)
}
