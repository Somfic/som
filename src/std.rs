pub struct BundledFile {
    pub name: &'static str,
    pub content: &'static str,
}

pub struct BundledModule {
    pub name: &'static str,
    pub files: &'static [BundledFile],
}

pub const BUNDLED_MODULES: &[BundledModule] = &[BundledModule {
    name: "std",
    files: &[
        BundledFile {
            name: "io.som",
            content: include_str!("../std/io.som"),
        },
        BundledFile {
            name: "mem.som",
            content: include_str!("../std/mem.som"),
        },
        BundledFile {
            name: "process.som",
            content: include_str!("../std/process.som"),
        },
    ],
}];

pub fn get_bundled_module(name: &str) -> Option<&'static BundledModule> {
    BUNDLED_MODULES.iter().find(|m| m.name == name)
}
