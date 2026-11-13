//! The standard library, compiled into the driver: every `.som` file under `std/`, in a module
//! named after its folder (`std/data/` is `som::data`). `use som;` loads all of them.

pub struct BundledModule {
    pub name: &'static str,
    pub files: &'static [(&'static str, &'static str)],
}

// Written by `build.rs` from what's in `std/`.
include!(concat!(env!("OUT_DIR"), "/bundled.rs"));

pub fn bundled(name: &str) -> Option<&'static BundledModule> {
    BUNDLED.iter().find(|m| m.name == name)
}

/// The bundled modules right inside `name`: `som::data` and `som::io` for `som`.
pub fn submodules(name: &str) -> impl Iterator<Item = &'static BundledModule> {
    let prefix = format!("{name}::");
    BUNDLED.iter().filter(move |m| {
        m.name
            .strip_prefix(&prefix)
            .is_some_and(|rest| !rest.contains("::"))
    })
}
