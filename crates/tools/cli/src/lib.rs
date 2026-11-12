//! The `som` command line interface.

mod ui;

use clap::builder::styling::{Color, RgbColor, Style, Styles};
use clap::{Args, Parser, Subcommand, ValueEnum};
use som_base::diagnostics::{BLUE, MAUVE, PEACH, RED, Rgb, SUBTEXT0, TEAL, YELLOW};
use som_base::message;
use som_base::{Diagnostic, msg};
use som_driver::{BuildOptions, Checked, Error, Options};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "som", version, about = "The som compiler", styles = STYLES)]
#[command(after_help = "A <PATH> is a .som file, or a directory of them.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Help and error colours, from the same Catppuccin palette as diagnostics, in the
/// same roles: keywords for headers, function names for commands and flags, types for
/// placeholders (in italics, to set them apart from literal text). Clap leaves the
/// colours out when output isn't a terminal or `NO_COLOR` is set.
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

// Subcommands are listed in `--help` in the order they're declared here.
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
    /// Compile a program to an executable
    Build {
        /// The program: a .som file, or a directory of them
        path: PathBuf,
        /// Where to write the executable [default: <NAME> for <NAME>.som, <DIR>/<DIR> for a directory]
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
}

#[derive(Args)]
struct BuildArgs {
    #[command(flatten)]
    check: CheckArgs,
    /// The C compiler to use
    #[arg(long, env = "SOM_CC", default_value = "cc", value_name = "PATH")]
    cc: String,
    /// Print the C compiler command before running it
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Args)]
struct CheckArgs {
    /// Also look for libraries in DIR, before the system's directories (repeatable)
    #[arg(short = 'L', long = "library-path", value_name = "DIR")]
    library_dirs: Vec<PathBuf>,

    /// Also write intermediate stages: tokens, ast, mir, c.
    ///
    /// Each goes next to the output by default (`main.c`). Write `c=out.c` for
    /// another file, or `c=-` for standard output. Separate several with commas:
    /// `--emit ast,mir,c`.
    #[arg(long, value_name = "STAGES", value_delimiter = ',', value_parser = parse_emit)]
    emit: Vec<Emit>,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Stage {
    Tokens,
    Ast,
    Mir,
    C,
}

impl Stage {
    fn extension(self) -> &'static str {
        match self {
            Stage::Tokens => "tokens",
            Stage::Ast => "ast",
            Stage::Mir => "mir",
            Stage::C => "c",
        }
    }
}

#[derive(Clone)]
struct Emit {
    stage: Stage,
    /// `None` for the default file, `-` for standard output.
    to: Option<PathBuf>,
}

fn parse_emit(value: &str) -> Result<Emit, String> {
    let (stage, to) = match value.split_once('=') {
        Some((stage, to)) => (stage, Some(PathBuf::from(to))),
        None => (value, None),
    };
    let stage = Stage::from_str(stage, true)
        .map_err(|_| format!("unknown stage `{stage}`; expected tokens, ast, mir or c"))?;
    Ok(Emit { stage, to })
}

/// Runs the CLI with `args` (including the program name) and returns the process exit code.
pub fn main(args: impl IntoIterator<Item = String>) -> ExitCode {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            return ExitCode::from(e.exit_code() as u8);
        }
    };
    som_driver::enable_highlighting();

    match cli.command {
        Command::Run { path, build, args } => run(&path, &build, &args),
        Command::Build {
            path,
            output,
            build,
        } => {
            let output = output.unwrap_or_else(|| default_output(&path));
            match compile(&path, &build.check, &output) {
                Some(checked) => {
                    build_to(&checked, &output, &build).map_or(ExitCode::FAILURE, |exe| {
                        ui::success("built", message::path(exe.display()));
                        ExitCode::SUCCESS
                    })
                }
                None => ExitCode::FAILURE,
            }
        }
        Command::Lsp { .. } => {
            som_lsp::serve();
            ExitCode::SUCCESS
        }
        Command::Check { path, check } => match compile(&path, &check, &default_output(&path)) {
            Some(_) => {
                ui::success("checked", message::path(path.display()));
                ExitCode::SUCCESS
            }
            None => ExitCode::FAILURE,
        },
    }
}

/// Loads and checks a program, writing whichever requested stages it gets to and
/// printing its errors. `None` if there were any.
///
/// Tokens and the AST are written even when later checks fail, which is when
/// they're most useful.
fn compile(path: &Path, args: &CheckArgs, output: &Path) -> Option<Checked> {
    let emits = &args.emit;
    if !path.exists() {
        ui::diagnostic(&missing_program(path));
        return None;
    }

    let (ast, errors) = som_driver::load(path);
    let mut ok = true;
    for e in emits {
        ok &= match e.stage {
            Stage::Tokens => write(e, output, &som_driver::dump_tokens(&ast)),
            Stage::Ast => write(e, output, &ast.dump()),
            Stage::Mir | Stage::C => true,
        };
    }
    if !errors.is_empty() {
        report(path, errors.into_iter().map(Error::Load).collect());
        return None;
    }

    let options = Options {
        library_dirs: args.library_dirs.clone(),
    };
    let checked = match som_driver::check_loaded(ast, &options) {
        Ok(checked) => checked,
        Err(errors) => {
            report(path, errors);
            return None;
        }
    };
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
            Stage::Tokens | Stage::Ast => true,
        };
    }
    ok.then_some(checked)
}

/// The error for a program path that doesn't exist, with a way forward.
fn missing_program(path: &Path) -> Diagnostic {
    let error = Diagnostic::error(msg![message::path(path.display()), " does not exist"]);
    let with_extension = path.with_extension("som");
    if path.extension().is_none() && with_extension.is_file() {
        return error.with_hint(msg![
            "did you mean ",
            message::path(with_extension.display()),
            "?"
        ]);
    }
    let name = path
        .file_stem()
        .map_or("main".into(), |s| s.to_string_lossy());
    let dir = path.with_extension("");
    error.with_hint(msg![
        "create ",
        message::path(dir.with_file_name(format!("{name}.som")).display()),
        ", or a directory ",
        message::path(format!("{}/", dir.display())),
        " with a ",
        message::path("main.som"),
        " in it",
    ])
}

/// Writes one emitted stage to its file, or to standard output for `-`.
fn write(emit: &Emit, output: &Path, contents: &str) -> bool {
    let path = emit
        .to
        .clone()
        .unwrap_or_else(|| output.with_extension(emit.stage.extension()));
    if path.as_os_str() == "-" {
        print!("{contents}");
        return true;
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

/// Shows a program's errors, then a summary like rustc's.
fn report(path: &Path, errors: Vec<Error>) {
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

/// Where a program's executable goes by default: `main.som` builds `./main`, like
/// `rustc main.rs`; a directory `example` builds `example/example`, since `./example`
/// is the program itself.
fn default_output(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if canonical.is_dir() {
        let name = canonical.file_name().unwrap_or("program".as_ref());
        path.join(name)
    } else {
        PathBuf::from(canonical.file_stem().unwrap_or("program".as_ref()))
    }
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
    let options = BuildOptions {
        cc: args.cc.clone(),
        on_command: args
            .verbose
            .then_some(|command: &str| ui::success("running", message::command(command))),
    };
    som_driver::build(checked, output, &options)
        .map_err(|e| ui::diagnostic(&e.to_diagnostic()))
        .ok()
}

fn run(path: &Path, build: &BuildArgs, args: &[String]) -> ExitCode {
    let Some(checked) = compile(path, &build.check, &default_output(path)) else {
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
    let code = run_in(&checked, path, &dir, build, args);
    let _ = std::fs::remove_dir_all(&dir);
    code
}

/// Builds the program into `dir` and runs it, returning its exit code.
fn run_in(
    checked: &Checked,
    path: &Path,
    dir: &Path,
    build: &BuildArgs,
    args: &[String],
) -> ExitCode {
    let Some(exe) = build_to(checked, &dir.join("program"), build) else {
        return ExitCode::FAILURE;
    };
    // The executable is temporary, so name the program rather than where it went.
    ui::success("built", message::path(path.display()));
    match som_driver::run(checked, &exe, args) {
        // Exit codes are a byte; a program killed by a signal has none.
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(e) => {
            ui::error(format!("could not start the program: {e}"));
            ExitCode::FAILURE
        }
    }
}
