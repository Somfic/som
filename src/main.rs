use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use som::{CompileOptions, CompileResult, EmitSet, Source};

// cli args
#[derive(clap::Parser)]
#[command(
    name = "som",
    version,
    about = "The som programming language",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a file (or the current project).
    Run {
        #[command(flatten)]
        input: InputArgs,
        #[command(flatten)]
        codegen: CodegenArgs,
    },
    /// Check a file (or the current project) without running.
    Check {
        #[command(flatten)]
        input: InputArgs,
    },
    /// Compile the current project.
    Build {
        #[command(flatten)]
        codegen: CodegenArgs,
    },
    /// Start the interactive REPL.
    Repl,
    /// Run the language server over stdio (for editor integration).
    Lsp {
        /// Accepted for editor compatibility; stdio is the only transport.
        #[arg(long, hide = true)]
        stdio: bool,
    },
    /// Create a new project.
    New { name: String },
    /// Remove the build directory.
    Clean,
}

#[derive(Clone, Copy, ValueEnum)]
enum Stage {
    Ast,
    Hir,
    Mir,
}

#[derive(Args, Default)]
struct InputArgs {
    /// Source file, `-` for stdin, or omitted for the project's src/main.som.
    input: Option<PathBuf>,
    #[command(flatten)]
    emit: EmitArgs,
}

#[derive(Args, Default)]
struct CodegenArgs {
    /// Optimization level (0–3).
    #[arg(
        short = 'O',
        long = "opt-level",
        value_name = "LEVEL",
        default_value_t = 0,
        value_parser = clap::value_parser!(u8).range(0..=3)
    )]
    opt_level: u8,
}

#[derive(Args, Default)]
struct EmitArgs {
    /// Dump compilation stages (comma-separated).
    #[arg(long, value_delimiter = ',')]
    emit: Vec<Stage>,
    /// Annotate dumps with the originating source lines.
    #[arg(long)]
    spans: bool,
}

impl EmitArgs {
    fn to_set(&self) -> EmitSet {
        let mut set = EmitSet {
            spans: self.spans,
            ..Default::default()
        };
        for stage in &self.emit {
            match stage {
                Stage::Ast => set.ast = true,
                Stage::Hir => set.hir = true,
                Stage::Mir => set.mir = true,
            }
        }
        set
    }
}

pub fn main() -> ExitCode {
    let cli = Cli::parse();

    // In LSP mode stdout is the JSON-RPC channel; installing a stdout tracing
    // subscriber would corrupt it, so leave tracing uninstalled there.
    if !matches!(cli.command, Command::Lsp { .. }) {
        som::init_tracing();
    }

    match cli.command {
        Command::Run { input, codegen } => {
            execute(build_options(Some(&input), Some(&codegen), true), None)
        }
        Command::Check { input } => {
            execute(build_options(Some(&input), None, false), Some("Checked."))
        }
        Command::Build { codegen } => execute(
            build_options(None, Some(&codegen), false),
            Some("Compiled project."),
        ),
        Command::Repl => repl(),
        Command::Lsp { .. } => cmd_lsp(),
        Command::New { name } => cmd_new(&name),
        Command::Clean => cmd_clean(),
    }
}

/// Assemble `CompileOptions` from the CLI flag groups a command supplied.
/// Fallible because resolving the source does I/O. `input` is absent for
/// project-only commands (build); `codegen` is absent for non-codegen ones (check).
fn build_options(
    input: Option<&InputArgs>,
    codegen: Option<&CodegenArgs>,
    run: bool,
) -> Result<CompileOptions, String> {
    let (path, emit) = match input {
        Some(args) => (args.input.as_deref(), args.emit.to_set()),
        None => (None, EmitSet::default()),
    };
    Ok(CompileOptions {
        input: resolve_input(path)?,
        emit,
        run,
        opt_level: codegen.map_or(0, |c| c.opt_level),
    })
}

fn execute(opts: Result<CompileOptions, String>, ok_msg: Option<&str>) -> ExitCode {
    let opts = match opts {
        Ok(opts) => opts,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::FAILURE;
        }
    };
    let result = som::compile(&opts);
    let ok = report(&result, opts.run);
    if ok && let Some(msg) = ok_msg {
        println!("{msg}");
    }
    exit(ok)
}

fn report(result: &CompileResult<i64>, run: bool) -> bool {
    render_diagnostics(result);
    if result.has_errors() {
        return false;
    }
    if run && let Some(value) = result.artifact {
        println!("{value}");
    }
    true
}

fn exit(ok: bool) -> ExitCode {
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn repl() -> ExitCode {
    use std::io::{BufRead, Write};

    let stdin = std::io::stdin();
    let mut handle = stdin.lock();
    let mut line = String::new();

    loop {
        print!(">>> ");
        let _ = std::io::stdout().flush();

        line.clear();
        if let Err(e) = handle.read_line(&mut line) {
            eprintln!("error: failed to read input: {e}");
            return ExitCode::FAILURE;
        }
        if line.trim().is_empty() {
            return ExitCode::SUCCESS;
        }

        let result = som::compile(&CompileOptions::new(Source::from_raw(line.clone())));
        report(&result, true);
    }
}

fn resolve_input(input: Option<&Path>) -> Result<Source, String> {
    match input {
        Some(p) if p.as_os_str() == "-" => read_stdin().map(Source::from_raw),
        Some(p) => Source::from_file(p).map_err(|e| format!("cannot read `{}`: {e}", p.display())),
        None => {
            let entry = Path::new("src/main.som");
            if entry.exists() {
                Source::from_file(entry)
                    .map_err(|e| format!("cannot read `{}`: {e}", entry.display()))
            } else {
                Err("no input and no `src/main.som` found".into())
            }
        }
    }
}

fn read_stdin() -> Result<String, String> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| format!("failed to read stdin: {e}"))?;
    Ok(buf)
}

fn render_diagnostics(result: &CompileResult<i64>) {
    for d in &result.diagnostics {
        eprint!("{}", som::render_diagnostic(d, &result.sources));
    }
}

fn cmd_new(name: &str) -> ExitCode {
    use std::fs;

    let root = PathBuf::from(name);
    if root.exists() {
        eprintln!("error: `{name}` already exists");
        return ExitCode::FAILURE;
    }
    if let Err(e) = fs::create_dir_all(root.join("src")) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    let manifest = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n");
    if fs::write(root.join("som.toml"), manifest).is_err()
        || fs::write(root.join("src/main.som"), "1 + 1\n").is_err()
    {
        eprintln!("error: failed to write project files");
        return ExitCode::FAILURE;
    }

    println!("Created project `{name}`");
    ExitCode::SUCCESS
}

fn cmd_lsp() -> ExitCode {
    match som_lsp::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: language server failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_clean() -> ExitCode {
    match std::fs::remove_dir_all("target") {
        Ok(()) => {
            println!("Removed target/");
            ExitCode::SUCCESS
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("Nothing to clean.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
