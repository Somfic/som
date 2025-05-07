use crate::{
    compiler::Compiler, parser::Parser, runner::Runner, tokenizer::Tokenizer, typer::Typer,
};

pub type Result<T> = std::result::Result<T, Vec<miette::MietteDiagnostic>>;
pub type ReportResult<T> = std::result::Result<T, Vec<miette::Report>>;

pub fn run(source_code: &'static str) -> i64 {
    let parser = Parser::new(Tokenizer::new(source_code));
    let parsed = parser
        .parse()
        .map_err(|err| print_errors_with_codebase(err, source_code))
        .expect("failed to parse codebase");

    let typer = Typer::new(parsed);
    let typed = typer
        .type_check()
        .map_err(|err| print_errors_with_codebase(err, source_code))
        .expect("failed to type check codebase");

    let compiler = Compiler::new(typed);
    let compiled = compiler
        .compile()
        .map_err(print_errors)
        .expect("failed to compile codebase");

    let runner = Runner::new(compiled);
    

    runner
        .run()
        .map_err(print_errors)
        .expect("failed to run codebase")
}

fn print_errors_with_codebase(errors: Vec<miette::MietteDiagnostic>, source_code: &'static str) {
    for error in errors {
        eprintln!("{:?}", miette::miette!(error).with_source_code(source_code));
    }
}

fn print_errors(errors: Vec<miette::Report>) {
    for error in errors {
        eprintln!("{:?}", error);
    }
}
