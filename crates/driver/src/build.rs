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
    /// The program has no `main` yet: likely the first thing someone sees, in an empty file.
    /// `end` is where to start one: the end of the program's own file.
    NoMain {
        end: Option<som_base::Span>,
    },
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
            BuildError::NoMain { end } => no_main(end.as_ref()),
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
    som_codegen::emit_c(&checked.mir).map_err(|e| match e {
        CodegenError::NoEntryPoint => BuildError::NoMain {
            end: program_file(checked).map(|file| {
                let end = file.content().len();
                som_base::Span::from_range(end..end, file)
            }),
        },
    })
}

/// The program's own file, where its `main` would go: its `main.som`, or its first file.
fn program_file(checked: &Checked) -> Option<std::sync::Arc<som_base::Source>> {
    let root = checked.ast.modules.iter().find(|m| m.name.is_empty())?;
    let named_main = root.files.iter().find(|f| {
        Path::new(f.identifier())
            .file_name()
            .and_then(|n| n.to_str())
            == Some("main.som")
    });
    named_main.or(root.files.first()).cloned()
}

/// A welcome, rather than only an error: a program with no `main` is most likely someone's
/// very first, so it says how a program starts, and offers a first line.
fn no_main(end: Option<&som_base::Span>) -> Diagnostic {
    // The file to write in, by its name: `main.som`.
    let file = end
        .and_then(|at| Path::new(at.source.identifier()).file_name())
        .map(|name| m::code(name.to_string_lossy()));
    let d = Diagnostic::error(msg!["the program has no entrypoint"])
        .with_hint(msg!["welcome to som!"])
        .with_hint(msg![
            "to get started, write your first lines in ",
            file.unwrap_or_else(|| "a `.som` file".into()),
            ", like ",
            m::code("println(\"Hello, world!\");")
        ])
        .with_hint("")
        .with_hint(msg!["happy hacking :)"]);
    let Some(at) = end else {
        return d;
    };
    // Appended as lines of their own, after what's there.
    let content = at.source.content();
    let lead = if content.trim().is_empty() || content.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let gap = if content.trim().is_empty() { "" } else { "\n" };
    d.with_fix(som_base::Fix {
        title: msg!["print a ", m::code("\"Hello, world!\"")],
        edits: vec![(
            at.clone(),
            format!("{lead}{gap}use som::io;\n\nprintln(\"Hello, world!\");\n"),
        )],
    })
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

    // Link the exact files that were checked, rather than letting the linker search.
    for library in &checked.libraries {
        cmd.arg(library);
    }
    // A static library (`libbox2d.a`) doesn't bring the libraries it uses, and libm,
    // separate from libc outside macOS, is the usual one.
    if !cfg!(target_os = "macos") {
        cmd.arg("-lm");
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
    let dirs: Vec<PathBuf> = checked
        .libraries
        .iter()
        .filter_map(|lib| lib.parent().map(Path::to_path_buf))
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
