//! Every shader in `corpus/shaders` that compiles is GLSL a real compiler takes: checked
//! with `glslc` (from the Vulkan SDK or shaderc), and skipped where it isn't installed.

use std::path::Path;
use std::process::Command;

#[test]
fn shaders_are_valid_glsl() {
    if Command::new("glslc").arg("--version").output().is_err() {
        eprintln!("skipped: `glslc` isn't installed");
        return;
    }
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/shaders");
    let dir = tempfile::tempdir().unwrap();
    let mut checked = 0;
    for entry in std::fs::read_dir(&corpus).unwrap() {
        let path = entry.unwrap().path();
        let source = std::fs::read_to_string(&path).unwrap();
        if !source.starts_with("// expect: exit") && !source.starts_with("// expect: ok") {
            continue;
        }
        // As `@glsl` sees it: before the optimizer shakes out what only it uses.
        let options = som_driver::Options {
            no_optimize: true,
            ..Default::default()
        };
        let program = som_driver::check(&path, &options)
            .unwrap_or_else(|_| panic!("{} doesn't compile", path.display()));
        for &shader in program.mir.stages.keys() {
            let glsl = som_shader::glsl(&program.mir, shader).unwrap_or_else(|errors| {
                panic!("{}: {}", path.display(), errors[0].to_diagnostic())
            });
            let file = dir.path().join(format!("shader{checked}.frag"));
            std::fs::write(&file, &glsl).unwrap();
            let out = Command::new("glslc")
                .args(["--target-env=opengl", "-fauto-map-locations", "-o"])
                .arg(dir.path().join("out.spv"))
                .arg(&file)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}: glslc rejected the GLSL:\n{}\n{glsl}",
                path.display(),
                String::from_utf8_lossy(&out.stderr)
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no shaders were checked");
}
