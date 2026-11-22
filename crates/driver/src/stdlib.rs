//! The standard library: the `.som` files under a `std/` folder, read from disk like any
//! other module, each folder a module named after it. `std/` itself is module `som`,
//! `std/data/` is `som::data`, and so on. `use som;` loads all of them.
//!
//! The folder is found when the compiler runs, not built into it, so changes to it apply
//! straight away, and editors can show its files. It's the first of:
//!
//! 1. `$SOM_STD`, to point at one explicitly;
//! 2. `lib/som/std` next to the folder the compiler is in (`bin/som` → `lib/som/std`),
//!    which is where a release puts it;
//! 3. the `std/` folder of the source tree the compiler was built from, for working on
//!    the compiler and the standard library themselves.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub use som_lang::{PRELUDE_MODULE as ROOT_MODULE, is_std_module};

/// The standard library's folder, if it can be found.
pub fn root() -> Option<&'static Path> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        candidates()
            .into_iter()
            .find_map(|dir| dir.canonicalize().ok())
    })
    .as_deref()
}

/// Where the standard library is looked for, in order.
pub fn candidates() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("SOM_STD") {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(bin) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        dirs.push(bin.join("../lib/som/std"));
    }
    dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std"));
    dirs
}

/// The folder of standard library module `name` (`som::data` → `std/data/`), if it's one
/// and exists.
pub fn folder(name: &str) -> Option<PathBuf> {
    if !is_std_module(name) {
        return None;
    }
    let rest = name.strip_prefix(ROOT_MODULE).expect("checked above");
    let folder = rest
        .split("::")
        .filter(|s| !s.is_empty())
        .fold(root()?.to_path_buf(), |p, s| p.join(s));
    folder.is_dir().then_some(folder)
}

/// The modules right inside `name`, one per folder: `som::data` and `som::io` for `som`.
pub fn submodules(name: &str) -> Vec<String> {
    let Some(folder) = folder(name) else {
        return Vec::new();
    };
    let mut children: Vec<String> = std::fs::read_dir(folder)
        .map(|dir| {
            dir.flatten()
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| is_module_name(n))
                .map(|n| format!("{name}::{n}"))
                .collect()
        })
        .unwrap_or_default();
    children.sort();
    children
}

/// Where libraries are looked for, after the program's own folders: each folder in
/// `$SOM_PATH` (separated by `:`), then `libs/` beside the standard library, the libraries
/// that come with som (`libs/wgpu`). A library is a folder of `.som` files, as a module of
/// the program's own is, and `use wgpu;` loads it.
pub fn library_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = std::env::var_os("SOM_PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    if let Some(libs) = root().and_then(Path::parent).map(|p| p.join("libs")) {
        roots.push(libs);
    }
    roots.retain(|r| r.is_dir());
    roots
}

/// The folder of library `name` (`wgpu`, or `wgpu::text` inside it), if a library root has
/// one.
pub fn library_folder(name: &str) -> Option<PathBuf> {
    if is_std_module(name) {
        return None;
    }
    library_roots().into_iter().find_map(|root| {
        let folder = name.split("::").fold(root, |p, s| p.join(s));
        folder.is_dir().then_some(folder)
    })
}

/// Whether a folder called `name` is a module: named like one, which `.build/`, where
/// building puts what it makes, isn't.
fn is_module_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Every module in the standard library, `som` first.
pub fn modules() -> Vec<String> {
    let mut all = Vec::new();
    let mut next = vec![ROOT_MODULE.to_string()];
    while let Some(name) = next.pop() {
        if folder(&name).is_none() {
            continue;
        }
        let mut children = submodules(&name);
        children.reverse();
        next.extend(children);
        all.push(name);
    }
    all
}

/// Whether `path` is a file or folder in the standard library.
pub fn contains(path: &Path) -> bool {
    let Some(root) = root() else { return false };
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_folders_named_like_modules_are_modules() {
        assert!(is_module_name("data"));
        assert!(is_module_name("io_2"));
        assert!(!is_module_name(".build"));
        assert!(!is_module_name(".git"));
        assert!(!is_module_name("2d"));
        assert!(!is_module_name("my-lib"));
    }
}
