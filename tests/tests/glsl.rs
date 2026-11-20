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
        let program = som_driver::check(&path, &Default::default())
            .unwrap_or_else(|_| panic!("{} doesn't compile", path.display()));
        for shader in &program.shaders {
            let file = dir.path().join(format!("{}.frag", shader.name));
            std::fs::write(&file, &shader.glsl).unwrap();
            let out = Command::new("glslc")
                .args(["--target-env=opengl", "-fauto-map-locations", "-o"])
                .arg(dir.path().join("out.spv"))
                .arg(&file)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}: glslc rejected `{}`:\n{}\n{}",
                path.display(),
                shader.name,
                String::from_utf8_lossy(&out.stderr),
                shader.glsl
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no shaders were checked");
}
