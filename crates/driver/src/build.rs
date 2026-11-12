//! Turns checked MIR into an executable, via C and the system C compiler.

use som_base::message as m;
use som_base::{Diagnostic, msg};
use som_codegen::CodegenError;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::Checked;

#[derive(Debug)]
pub enum BuildError {
    Codegen(CodegenError),
    Io {
        path: PathBuf,
        message: String,
    },
    /// No C compiler could be started.
    NoCompiler {
        compiler: String,
        message: String,
    },
    /// The C compiler rejected the generated code. That's a bug in codegen.
    CompilerFailed {
        stderr: String,
        c_file: PathBuf,
    },
}

impl BuildError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            BuildError::Codegen(e) => Diagnostic::error(e.to_string()),
            BuildError::Io { path, message } => Diagnostic::error(msg![
                "could not write ",
                m::path(path.display()),
                format!(": {message}")
            ]),
            BuildError::NoCompiler { compiler, message } => Diagnostic::error(msg![
                "could not run the C compiler ",
                m::command(compiler),
                format!(": {message}")
            ])
            .with_hint(msg![
                "install one (on macOS: ",
                m::command("xcode-select --install"),
                "), or name one with ",
                m::command("--cc"),
                " or ",
                m::command("SOM_CC"),
            ]),
            BuildError::CompilerFailed { stderr, c_file } => {
                let mut diagnostic =
                    Diagnostic::error("the C compiler rejected the generated code");
                // One line per line of the compiler's output, so it stays readable.
                diagnostic.trace = stderr.trim().lines().map(str::to_string).collect();
                diagnostic
                    .with_hint(msg![
                        "the generated C is kept at ",
                        m::path(c_file.display())
                    ])
                    .with_hint("this is a bug in the som compiler")
            }
        }
    }
}

/// How to turn generated C into an executable.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// The C compiler to run.
    pub cc: String,
    /// Called with the C compiler command line before it runs, to show it.
    pub on_command: Option<fn(&str)>,
}

impl Default for BuildOptions {
    /// `$SOM_CC`, or `cc`.
    fn default() -> Self {
        BuildOptions {
            cc: std::env::var("SOM_CC").unwrap_or_else(|_| "cc".into()),
            on_command: None,
        }
    }
}

/// The C source for a checked program.
pub fn emit_c(checked: &Checked) -> Result<String, BuildError> {
    som_codegen::emit_c(&checked.mir).map_err(BuildError::Codegen)
}

/// Compiles a checked program to an executable at `output`. The generated C goes
/// to a temporary file, which is kept only if the C compiler rejects it.
pub fn build(
    checked: &Checked,
    output: &Path,
    options: &BuildOptions,
) -> Result<PathBuf, BuildError> {
    let c = emit_c(checked)?;
    // Unique per build: several builds can run at once in one process, like the corpus tests.
    static BUILDS: AtomicUsize = AtomicUsize::new(0);
    let n = BUILDS.fetch_add(1, Ordering::Relaxed);
    let stem = output
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("program");
    let c_file = std::env::temp_dir().join(format!("som-{}-{n}-{stem}.c", std::process::id()));
    std::fs::write(&c_file, c).map_err(|e| BuildError::Io {
        path: c_file.clone(),
        message: e.to_string(),
    })?;

    let compiler = options.cc.clone();
    let mut cmd = Command::new(&compiler);
    cmd.args(["-std=c11", "-O1", "-w"])
        // som integers wrap on overflow, and som's extern declarations are the only ones that count.
        .args(["-fwrapv", "-fno-builtin"])
        .arg("-o")
        .arg(output)
        .arg(&c_file);

    let (libraries, _) = checked.ast.extern_libraries();
    for path in library_search_paths() {
        cmd.arg(format!("-L{path}"));
    }
    for lib in &libraries {
        if is_library_path(lib) {
            cmd.arg(lib);
        } else {
            cmd.arg(format!("-l{lib}"));
        }
    }

    if let Some(show) = options.on_command {
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        show(&format!("{compiler} {}", args.join(" ")));
    }
    let result = cmd.output().map_err(|e| BuildError::NoCompiler {
        compiler: compiler.clone(),
        message: e.to_string(),
    })?;
    if !result.status.success() {
        return Err(BuildError::CompilerFailed {
            stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
            c_file,
        });
    }
    let _ = std::fs::remove_file(&c_file);
    Ok(output.to_path_buf())
}

/// Runs a built program with `args`, making the libraries it was linked against findable.
pub fn run(checked: &Checked, executable: &Path, args: &[String]) -> std::io::Result<ExitStatus> {
    let mut cmd = Command::new(executable);
    cmd.args(args);
    let (libraries, _) = checked.ast.extern_libraries();
    let dirs: Vec<PathBuf> = libraries
        .iter()
        .filter(|lib| is_library_path(lib))
        .filter_map(|lib| Path::new(lib).parent().map(Path::to_path_buf))
        .collect();
    if !dirs.is_empty() {
        let joined = std::env::join_paths(&dirs).unwrap_or_default();
        let var = if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        cmd.env(var, joined);
    }
    cmd.status()
}

/// `extern "path/to/libfoo.so"` names a file; `extern "foo"` names a library to search for.
fn is_library_path(lib: &str) -> bool {
    lib.contains('/')
        || [".so", ".dylib", ".a", ".dll"]
            .iter()
            .any(|ext| lib.ends_with(ext))
}

fn library_search_paths() -> Vec<&'static str> {
    if cfg!(target_arch = "aarch64") && cfg!(target_os = "macos") {
        vec!["/opt/homebrew/lib"]
    } else {
        vec!["/usr/local/lib"]
    }
}
