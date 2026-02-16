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
            name: "alloc.som",
            content: include_str!("../../std/alloc.som"),
        },
        BundledFile {
            name: "io.som",
            content: include_str!("../../std/io.som"),
        },
        BundledFile {
            name: "mem.som",
            content: include_str!("../../std/mem.som"),
        },
        BundledFile {
            name: "process.som",
            content: include_str!("../../std/process.som"),
        },
        BundledFile {
            name: "string.som",
            content: include_str!("../../std/string.som"),
        },
    ],
}];

pub fn get_bundled_module(name: &str) -> Option<&'static BundledModule> {
    BUNDLED_MODULES.iter().find(|m| m.name == name)
}

impl BundledModule {
    /// Extract exported function and struct names by scanning source text.
    pub fn exported_names(&self) -> (Vec<&'static str>, Vec<&'static str>) {
        let mut functions = Vec::new();
        let mut structs = Vec::new();
        for file in self.files {
            // Track whether we're inside an extern block (depth of braces after "extern {")
            let mut in_extern = false;
            let mut brace_depth: i32 = 0;
            let mut extern_brace_start: i32 = 0;

            for line in file.content.lines() {
                let trimmed = line.trim();

                // Detect extern block start
                if trimmed.starts_with("extern") && trimmed.contains('{') {
                    in_extern = true;
                    extern_brace_start = brace_depth;
                    brace_depth += trimmed.matches('{').count() as i32;
                    brace_depth -= trimmed.matches('}').count() as i32;
                    continue;
                }

                // Track braces
                brace_depth += trimmed.matches('{').count() as i32;
                brace_depth -= trimmed.matches('}').count() as i32;

                // End of extern block
                if in_extern && brace_depth <= extern_brace_start {
                    in_extern = false;
                    continue;
                }

                // Top-level fn (not inside extern block)
                if !in_extern && trimmed.starts_with("fn ") {
                    if let Some(name) = trimmed
                        .strip_prefix("fn ")
                        .and_then(|rest| rest.split(|c: char| c == '(' || c == '<').next())
                    {
                        let name = name.trim();
                        if !name.is_empty() {
                            functions.push(name);
                        }
                    }
                }

                // struct
                if trimmed.starts_with("struct ") {
                    if let Some(name) = trimmed
                        .strip_prefix("struct ")
                        .and_then(|rest| rest.split(|c: char| c == '{' || c.is_whitespace()).next())
                    {
                        let name = name.trim();
                        if !name.is_empty() {
                            structs.push(name);
                        }
                    }
                }
            }
        }
        (functions, structs)
    }
}
