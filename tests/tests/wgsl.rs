//! Every shader in `corpus/shaders` that compiles is WGSL that wgpu takes: read and checked
//! by naga, wgpu's shader compiler.

use std::path::Path;

#[test]
fn shaders_are_valid_wgsl() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/shaders");
    let mut checked = 0;
    let mut failed = Vec::new();
    let mut paths: Vec<_> = (std::fs::read_dir(&corpus).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .filter(|path| {
            let source = std::fs::read_to_string(path).unwrap();
            source.starts_with("// expect: exit") || source.starts_with("// expect: ok")
        })
        .collect();
    paths.extend(std::env::var_os("SOM_WGSL_EXTRA").map(std::path::PathBuf::from));
    for path in paths {
        let program = som_driver::check(&path, &Default::default())
            .unwrap_or_else(|_| panic!("{} doesn't compile", path.display()));
        for shader in &program.shaders {
            let wgsl = &shader.wgsl;
            if let Err(problem) = check(wgsl) {
                failed.push(format!("{}: naga rejected `{}`:\n{problem}\n{wgsl}", path.display(), shader.name));
            }
            checked += 1;
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n\n"));
    assert!(checked > 0, "no shaders were checked");
    eprintln!("checked {checked} shaders");
}

fn check(wgsl: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(wgsl).map_err(|e| e.emit_to_string(wgsl))?;
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default())
        .validate(&module)
        .map_err(|e| e.emit_to_string(wgsl))?;
    Ok(())
}
