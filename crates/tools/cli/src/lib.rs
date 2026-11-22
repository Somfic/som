mod ui;

use clap::builder::styling::{Color, RgbColor, Style, Styles};
use clap::builder::{EnumValueParser, PossibleValue, TypedValueParser};
use clap::{Args, Parser, Subcommand, ValueEnum};
use som_base::diagnostics::{BLUE, MAUVE, PEACH, RED, Rgb, SUBTEXT0, TEAL, YELLOW};
use som_base::message;
use som_base::message::Message;
use som_base::{Diagnostic, msg};
use som_driver::toolchain::{self, C_COMPILER_VAR, DEFAULT_C_COMPILER, Target, Toolchain, Triple};
use som_driver::{BuildOptions, Checked, Error, Failed, Options};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::LazyLock;

#[derive(Parser)]
#[command(name = "som", version, about = "The som compiler", styles = STYLES)]
#[command(after_help = "A <PATH> is a .som file, or a directory of them.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// The diagnostics' palette, in the same roles: keywords for headers, function names for
/// commands and flags, types for placeholders (in italics, to set them apart from literal
/// text).
const STYLES: Styles = Styles::styled()
    .header(color(MAUVE).bold())
    .usage(color(MAUVE).bold())
    .literal(color(BLUE))
    .placeholder(color(YELLOW).italic())
    .error(color(RED).bold())
    .valid(color(TEAL))
    .invalid(color(PEACH))
    .context(color(SUBTEXT0))
    .context_value(color(PEACH));

const fn color(rgb: Rgb) -> Style {
    Style::new().fg_color(Some(Color::Rgb(RgbColor(rgb.0, rgb.1, rgb.2))))
}

// `--help` lists the subcommands in this order.
#[derive(Subcommand)]
enum Command {
    /// Compile a program and run it, exiting with its exit code
    Run {
        /// The program: a .som file, or a directory of them
        path: PathBuf,
        #[command(flatten)]
        build: BuildArgs,
        /// Arguments for the program, after `--`
        #[arg(last = true, value_name = "ARGS")]
        args: Vec<String>,
    },
    /// Run a program's `@test` functions, each in a process of its own
    Test {
        /// The program: a .som file, or a directory of them
        path: PathBuf,
        #[command(flatten)]
        build: BuildArgs,
    },
    /// Compile a program to an executable
    Build {
        /// The program: a .som file, or a directory of them
        path: PathBuf,
        /// Where to write the executable [default: .build/<NAME> next to the program, .build/<TRIPLE>/<NAME> for another system]
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
        #[command(flatten)]
        build: BuildArgs,
    },
    /// Check a program for errors without compiling it
    Check {
        /// The program: a .som file, or a directory of them
        path: PathBuf,
        #[command(flatten)]
        check: CheckArgs,
    },
    /// Start the language server, for editors (speaks LSP over stdin and stdout)
    Lsp {
        /// Accepted because editors pass it; stdin and stdout are the only transport
        #[arg(long, hide = true)]
        stdio: bool,
    },
    /// Regenerate the standard library's C library declarations for every system, with a
    /// C compiler that can compile for all of them. Only in debug builds: it's for working
    /// on som, and writes into the standard library.
    #[cfg(debug_assertions)]
    #[command(hide = true)]
    GenerateLibc {
        #[arg(long, env = C_COMPILER_VAR, default_value = "zig cc", value_name = "PATH")]
        cc: String,
    },
}

#[derive(Args)]
struct BuildArgs {
    #[command(flatten)]
    check: CheckArgs,
    /// Print the C compiler command before running it
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Args)]
struct CheckArgs {
    /// Also look for libraries in DIR, before the system's directories (repeatable)
    #[arg(short = 'L', long = "library-path", value_name = "DIR")]
    library_dirs: Vec<PathBuf>,

    /// The C compiler, which also reads imported headers (`zig cc` works)
    #[arg(long, env = C_COMPILER_VAR, default_value = DEFAULT_C_COMPILER, value_name = "PATH")]
    cc: String,

    /// Compile for another system: repeat it for several, each built into
    /// `.build/<TRIPLE>/`.
    ///
    /// The C compiler has to be able to cross-compile: clang with `--sysroot`, or `zig cc`,
    /// which brings the C libraries of every system. Libraries installed here aren't
    /// looked in, only `-L` directories and the sysroot's.
    #[arg(long = "target", value_name = "TRIPLE")]
    targets: Vec<TargetArg>,

    /// Where the target's headers and libraries are, given to the C compiler
    #[arg(long, value_name = "DIR", requires = "targets")]
    sysroot: Option<PathBuf>,

    /// Also write intermediate stages: tokens, ast, mir, c, wgsl.
    ///
    /// Each goes next to the output by default (`main.c`). Write `c=out.c` for
    /// another file, or `c=-` for standard output. Separate several with commas:
    /// `--emit ast,mir,c`.
    ///
    /// `wgsl` writes each `@fragment` and `@compute` function as the GPU gets it, a file
    /// each: `main.stripes.wgsl`. `wgsl=DIR` puts them in DIR; `wgsl=-` prints them all.
    #[arg(long, value_name = "STAGES", value_delimiter = ',', value_parser = EmitParser)]
    emit: Vec<Emit>,

    /// Skip the optimizer: no constant folding, and unused functions are kept.
    ///
    /// With `--emit mir,c`, to see the program as lowering made it. Constants are still
    /// worked out while compiling.
    #[arg(long)]
    no_optimize: bool,
}

/// A `--target`: one of the systems som can compile for, by name or another spelling.
#[derive(Clone, Copy)]
struct TargetArg(Triple);

impl ValueEnum for TargetArg {
    fn value_variants<'a>() -> &'a [Self] {
        static ALL: LazyLock<Vec<TargetArg>> =
            LazyLock::new(|| Triple::ALL.into_iter().map(TargetArg).collect());
        &ALL
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.0.name()).aliases(self.0.aliases().iter().copied()))
    }
}

/// The systems to compile for: each `--target`, or the one som runs on.
fn targets(args: &CheckArgs) -> Vec<Target> {
    if args.targets.is_empty() {
        return vec![Target::host()];
    }
    let mut targets: Vec<Target> = Vec::new();
    for &TargetArg(triple) in &args.targets {
        let target = Target::new(Some(triple), args.sysroot.clone());
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}

/// `example`, or `example for x86_64-windows-gnu` when that's not the system som runs on.
fn program_for(path: &Path, target: &Target) -> Message {
    match target.triple() {
        Some(triple) => msg![
            message::path(path.display()),
            " for ",
            message::code(triple)
        ],
        None => msg![message::path(path.display())],
    }
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Stage {
    Tokens,
    Ast,
    Mir,
    C,
    Wgsl,
}

impl Stage {
    fn extension(self) -> &'static str {
        match self {
            Stage::Tokens => "tokens",
            Stage::Ast => "ast",
            Stage::Mir => "mir",
            Stage::C => "c",
            Stage::Wgsl => "wgsl",
        }
    }
}

/// A stage's output goes to standard output when written to this.
const STDOUT: &str = "-";

#[derive(Clone)]
struct Emit {
    stage: Stage,
    /// `None` for the default file.
    to: Option<PathBuf>,
}

/// Parses an `--emit`: a stage, `c`, or a stage and where it goes, `c=out.c`. Its possible
/// values are the stages, as an enum's are, for `--help`, completion and suggestions.
#[derive(Clone)]
struct EmitParser;

impl TypedValueParser for EmitParser {
    type Value = Emit;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &OsStr,
    ) -> Result<Emit, clap::Error> {
        let text = value.to_string_lossy();
        let (stage, to) = match text.split_once('=') {
            Some((stage, to)) => (stage, Some(PathBuf::from(to))),
            None => (&*text, None),
        };
        let stage = EnumValueParser::<Stage>::new().parse_ref(cmd, arg, OsStr::new(stage))?;
        Ok(Emit { stage, to })
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        let stages = Stage::value_variants().iter();
        Some(Box::new(stages.filter_map(ValueEnum::to_possible_value)))
    }
}

/// With several targets, each writes its own files: an `--emit` to a named one can't be
/// for all of them.
fn emits_apart(args: &CheckArgs, targets: &[Target]) -> bool {
    let named =
        (args.emit.iter()).find_map(|e| e.to.as_ref().filter(|to| to.as_os_str() != STDOUT));
    let Some(named) = named.filter(|_| targets.len() > 1) else {
        return true;
    };
    ui::error(msg![
        message::command("--emit"),
        " writes ",
        message::path(named.display()),
        ", which each target would overwrite; without a file, each goes next to its executable"
    ]);
    false
}

/// `args` includes the program name.
pub fn main(args: impl IntoIterator<Item = String>) -> ExitCode {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            return ExitCode::from(e.exit_code() as u8);
        }
    };

    match cli.command {
        Command::Run { path, build, args } => run(&path, &build, &args, false),
        Command::Test { path, build } => run(&path, &build, &[], true),
        Command::Build {
            path,
            output,
            build,
        } => {
            let targets = targets(&build.check);
            if !emits_apart(&build.check, &targets) {
                return ExitCode::FAILURE;
            }
            if output.is_some() && targets.len() > 1 {
                ui::error(msg![
                    message::command("-o"),
                    " names one file, but each target needs its own; without it, each goes in ",
                    message::path(format!("{}/<TRIPLE>/", toolchain::BUILD_DIR)),
                ]);
                return ExitCode::FAILURE;
            }
            let mut ok = true;
            for target in &targets {
                let output = (output.clone()).unwrap_or_else(|| default_output(&path, target));
                let built = compile(&path, &build.check, target, &output, false)
                    .and_then(|checked| build_to(&checked, &output, &build));
                match built {
                    Some(exe) => ui::success("built", message::path(exe.display())),
                    None => ok = false,
                }
            }
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Command::Lsp { .. } => {
            som_lsp::serve();
            ExitCode::SUCCESS
        }
        #[cfg(debug_assertions)]
        Command::GenerateLibc { cc } => generate_libc(cc),
        Command::Check { path, check } => {
            let targets = targets(&check);
            if !emits_apart(&check, &targets) {
                return ExitCode::FAILURE;
            }
            let mut ok = true;
            for target in &targets {
                let output = default_output(&path, target);
                match compile(&path, &check, target, &output, false) {
                    Some(_) => ui::success("checked", program_for(&path, target)),
                    None => ok = false,
                }
            }
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

/// Loads and checks a program, writing whichever requested stages it gets to and
/// printing its errors. `None` if there were any. Tokens and the AST are written even
/// when later checks fail, which is when they're most useful. With `test`, the program is
/// its `@test`s instead of `main`.
fn compile(
    path: &Path,
    args: &CheckArgs,
    target: &Target,
    output: &Path,
    test: bool,
) -> Option<Checked> {
    let emits = &args.emit;
    if !path.exists() {
        ui::diagnostic(&missing_program(path));
        return None;
    }

    let toolchain = Toolchain {
        cc: args.cc.clone(),
        target: target.clone(),
        library_dirs: args.library_dirs.clone(),
    };
    let (ast, errors) = som_driver::load(path, &toolchain);
    let mut ok = true;
    for e in emits {
        ok &= match e.stage {
            Stage::Tokens => write(e, output, &som_driver::dump_tokens(&ast)),
            Stage::Ast => write(e, output, &ast.dump()),
            Stage::Mir | Stage::C | Stage::Wgsl => true,
        };
    }
    if !errors.is_empty() {
        report(
            path,
            Failed {
                errors: errors.into_iter().map(Error::Load).collect(),
                warnings: Vec::new(),
            },
        );
        return None;
    }

    let options = Options {
        toolchain,
        no_optimize: args.no_optimize,
        test,
        ..Options::default()
    };
    let checked = match som_driver::check_loaded(ast, &options) {
        Ok(checked) => checked,
        Err(failed) => {
            report(path, failed);
            return None;
        }
    };
    for warning in &checked.warnings {
        ui::diagnostic(&warning.to_diagnostic());
    }
    for e in emits {
        ok &= match e.stage {
            Stage::Mir => write(e, output, &checked.mir.to_string()),
            Stage::C => match som_driver::emit_c(&checked) {
                Ok(c) => write(e, output, &c),
                Err(error) => {
                    ui::diagnostic(&error.to_diagnostic());
                    false
                }
            },
            Stage::Wgsl => write_shaders(e, output, &checked.shaders),
            Stage::Tokens | Stage::Ast => true,
        };
    }
    ok.then_some(checked)
}

fn missing_program(path: &Path) -> Diagnostic {
    let error = Diagnostic::error(msg![message::path(path.display()), " does not exist"]);
    let with_extension = path.with_extension(toolchain::SOURCE_EXTENSION);
    if path.extension().is_none() && with_extension.is_file() {
        return error.with_hint(msg![
            "did you mean ",
            message::path(with_extension.display()),
            "?"
        ]);
    }
    let name = path
        .file_stem()
        .map_or(som_lang::ENTRY_POINT.into(), |s| s.to_string_lossy());
    let dir = path.with_extension("");
    let file = format!("{name}.{}", toolchain::SOURCE_EXTENSION);
    error.with_hint(msg![
        "create ",
        message::path(dir.with_file_name(file).display()),
        ", or a directory ",
        message::path(format!("{}/", dir.display())),
        " with a ",
        message::path(toolchain::MAIN_FILE),
        " in it",
    ])
}

/// Each shader in a file of its own, `main.stripes.wgsl`, or with `wgsl=-` all of them on
/// standard output, each after a line naming it.
fn write_shaders(emit: &Emit, output: &Path, shaders: &[som_driver::Shader]) -> bool {
    let source = |shader: &som_driver::Shader| shader.wgsl.clone();
    if emit.to.as_deref() == Some(Path::new(STDOUT)) {
        for shader in shaders {
            print!("// shader: {}\n{}\n", shader.name, source(shader));
        }
        return true;
    }
    let stem = output.file_stem().map_or("main".into(), |s| s.to_string_lossy());
    let mut ok = true;
    for shader in shaders {
        let file = format!("{stem}.{}.wgsl", shader.name);
        let path = match &emit.to {
            Some(dir) => dir.join(&file),
            None => output.with_file_name(&file),
        };
        let one = Emit {
            stage: emit.stage,
            to: Some(path),
        };
        ok &= write(&one, output, &source(shader));
    }
    ok
}

fn write(emit: &Emit, output: &Path, contents: &str) -> bool {
    let path = emit
        .to
        .clone()
        .unwrap_or_else(|| output.with_extension(emit.stage.extension()));
    if path.as_os_str() == STDOUT {
        print!("{contents}");
        return true;
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::write(&path, contents) {
        Ok(()) => {
            ui::success("wrote", message::path(path.display()));
            true
        }
        Err(e) => {
            ui::error(msg![
                "could not write ",
                message::path(path.display()),
                format!(": {e}")
            ]);
            false
        }
    }
}

/// A program's errors, then a summary like rustc's.
fn report(path: &Path, failed: Failed) {
    let Failed { errors, warnings } = failed;
    for warning in &warnings {
        ui::diagnostic(&warning.to_diagnostic());
    }
    for error in &errors {
        ui::diagnostic(&error.to_diagnostic());
    }
    let plural = if errors.len() == 1 { "" } else { "s" };
    ui::error(msg![
        "could not compile ",
        message::path(path.display()),
        format!(" due to {} error{plural}", errors.len()),
    ]);
}

/// Where a program's executable goes by default: in the build folder next to its
/// `main.som`, named after it: `example/build/example` for the directory `example`,
/// `.build/hello` for `hello.som`. Another system's go in `.build/<triple>/`.
fn default_output(path: &Path, target: &Target) -> PathBuf {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let (root, name) = if canonical.is_dir() {
        (path.to_path_buf(), canonical.file_name())
    } else {
        (som_driver::program_root(path), canonical.file_stem())
    };
    toolchain::build_dir(&root, target).join(name.unwrap_or("program".as_ref()))
}

fn build_to(checked: &Checked, output: &Path, args: &BuildArgs) -> Option<PathBuf> {
    if output.is_dir() {
        ui::diagnostic(
            &Diagnostic::error(msg![
                "cannot write the executable to ",
                message::path(output.display()),
                ": it is a directory",
            ])
            .with_hint(msg![
                "choose another name with ",
                message::command("-o <FILE>")
            ]),
        );
        return None;
    }
    if let Err(missing) = checked.toolchain.check_target() {
        ui::diagnostic(&missing.to_diagnostic());
        return None;
    }
    if let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty())
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        ui::error(msg![
            "could not create ",
            message::path(dir.display()),
            format!(": {e}")
        ]);
        return None;
    }
    let options = BuildOptions {
        on_command: args
            .verbose
            .then_some(|command: &str| ui::success("running", message::command(command))),
    };
    som_driver::build(checked, output, &options)
        .map_err(|e| ui::diagnostic(&e.to_diagnostic()))
        .ok()
}

/// Builds and runs the program, or with `test`, its tests.
fn run(path: &Path, build: &BuildArgs, args: &[String], test: bool) -> ExitCode {
    let targets = targets(&build.check);
    let [target] = &targets[..] else {
        let command = if test { "som test" } else { "som run" };
        ui::error(msg![
            message::command(command),
            " runs one program: give it one ",
            message::command("--target"),
            " at most"
        ]);
        return ExitCode::FAILURE;
    };
    let output = default_output(path, target);
    let Some(checked) = compile(path, &build.check, target, &output, test) else {
        return ExitCode::FAILURE;
    };
    let dir = std::env::temp_dir().join(format!("som-run-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        ui::error(msg![
            "could not create ",
            message::path(dir.display()),
            format!(": {e}")
        ]);
        return ExitCode::FAILURE;
    }
    let code = run_in(&checked, &dir, build, args);
    let _ = std::fs::remove_dir_all(&dir);
    code
}

fn run_in(checked: &Checked, dir: &Path, build: &BuildArgs, args: &[String]) -> ExitCode {
    let Some(exe) = build_to(checked, &dir.join("program"), build) else {
        return ExitCode::FAILURE;
    };
    match som_driver::run(checked, &exe, args) {
        // Exit codes are a byte; a program killed by a signal has none.
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(e) => {
            ui::error(format!("could not start the program: {e}"));
            ExitCode::FAILURE
        }
    }
}

/// Writes `std/c/<system>.som` for each of `LIBC_TARGETS`.
#[cfg(debug_assertions)]
fn generate_libc(cc: String) -> ExitCode {
    let Some(std) = som_driver::stdlib::root() else {
        ui::error("could not find the standard library");
        return ExitCode::FAILURE;
    };
    let dir = std.join(toolchain::LIBC_BINDINGS_DIR);
    let scratch = std::env::temp_dir().join(format!("som-libc-{}", std::process::id()));
    let mut ok = std::fs::create_dir_all(&dir).is_ok();
    for (os, triple) in toolchain::LIBC_TARGETS {
        let target = Target::new(Some(*triple), None);
        let toolchain = Toolchain {
            cc: cc.clone(),
            target,
            library_dirs: Vec::new(),
        };
        let path = dir.join(format!("{}.{}", os.name(), toolchain::SOURCE_EXTENSION));
        let written = som_driver::libc_bindings(&scratch, &toolchain)
            .and_then(|text| std::fs::write(&path, text).map_err(|e| e.to_string()));
        match written {
            Ok(()) => ui::success("wrote", message::path(path.display())),
            Err(e) => {
                ui::error(msg![
                    "could not generate ",
                    message::path(path.display()),
                    format!(": {e}")
                ]);
                ok = false;
            }
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
