//! The standard library, compiled into the driver and loaded by `use std;`.

pub struct BundledModule {
    pub name: &'static str,
    pub files: &'static [(&'static str, &'static str)],
}

pub const BUNDLED: &[BundledModule] = &[BundledModule {
    name: "std",
    files: &[
        ("alloc.som", include_str!("../../../std/alloc.som")),
        ("io.som", include_str!("../../../std/io.som")),
        ("mem.som", include_str!("../../../std/mem.som")),
        ("process.som", include_str!("../../../std/process.som")),
        ("string.som", include_str!("../../../std/string.som")),
    ],
}];

pub fn bundled(name: &str) -> Option<&'static BundledModule> {
    BUNDLED.iter().find(|m| m.name == name)
}
