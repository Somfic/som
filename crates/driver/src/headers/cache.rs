//! The generated source is kept in `.build/imports/<module>.som` next to the program, for reuse and
//! for reading: errors and go-to-definition point into it. Its first line says what it was
//! made from; while that still matches, the header doesn't need to be read again.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use crate::toolchain::{self, Target};

/// Bumped whenever the generated source changes, so old caches are regenerated.
const GENERATOR_VERSION: u32 = 29;

const ORIGINS_EXTENSION: &str = "origins";

pub(super) struct Cache {
    pub path: PathBuf,
    stamp: String,
}

impl Cache {
    pub fn new(
        root: &Path,
        target: &Target,
        module: &str,
        header: &Path,
        library: Option<&Path>,
    ) -> Cache {
        let file = format!("{module}.{}", toolchain::SOURCE_EXTENSION);
        Cache {
            path: toolchain::imports_dir(root, target).join(file),
            stamp: stamp(header, library),
        }
    }

    pub fn dir(&self) -> &Path {
        self.path.parent().expect("in .som")
    }

    pub fn read(&self) -> Option<String> {
        std::fs::read_to_string(&self.path)
            .ok()
            .filter(|text| text.lines().next() == Some(self.stamp.as_str()))
    }

    /// Stores freshly generated source, returning it as it's stored.
    pub fn write(&self, generated: &str, origins: &str) -> String {
        let text = format!("{}\n{generated}", self.stamp);
        // A cache that can't be written only costs time next build.
        let _ = std::fs::create_dir_all(self.dir())
            .and_then(|()| std::fs::write(&self.path, &text))
            .and_then(|()| std::fs::write(self.path.with_extension(ORIGINS_EXTENSION), origins));
        text
    }
}

/// Everything a cached import depends on. Any change to the header, the library or the
/// generator makes it stale.
fn stamp(header: &Path, library: Option<&Path>) -> String {
    let modified = |path: &Path| {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs())
    };
    let mut stamp = format!(
        "// som import v{GENERATOR_VERSION}.{}: {} @{}",
        env!("CARGO_PKG_VERSION"),
        header.display(),
        modified(header)
    );
    if let Some(library) = library {
        let _ = write!(stamp, ", {} @{}", library.display(), modified(library));
    }
    stamp
}

/// Where each function in a generated file (`.build/imports/raylib.som`) is declared in C: its
/// som name → the header and line, from the `.origins` file written next to it.
pub fn origins(generated: &Path) -> HashMap<String, (PathBuf, usize)> {
    let text =
        std::fs::read_to_string(generated.with_extension(ORIGINS_EXTENSION)).unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let (name, file, line) = (parts.next()?, parts.next()?, parts.next()?);
            Some((name.to_string(), (PathBuf::from(file), line.parse().ok()?)))
        })
        .collect()
}

/// A header including all of the target's C library headers, written only when it
/// changes so the import it's stamped into stays cached.
pub(super) fn libc_header(root: &Path, target: &Target) -> std::io::Result<PathBuf> {
    let path = toolchain::imports_dir(root, target).join("c.h");
    let text: String = (target.libc_headers().iter())
        .map(|h| format!("#include <{h}>\n"))
        .collect();
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::create_dir_all(path.parent().expect("in the cache dir"))?;
        std::fs::write(&path, text)?;
    }
    Ok(path)
}
