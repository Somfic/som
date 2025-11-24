use som::{
    ast::Expression, Diagnostic, Emitter, Linker, Parser, ProgramParser, Runner, Source, Typer,
};
use target_lexicon::Triple;

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
    }
}

fn run() -> Result<(), Diagnostic> {
    let source = Source::from_raw(
        "
    {
        use std::print;

        extern c puts as puts = fn(*byte) -> i32;

        pub mod let a = 1;
        while a > 0 {
            puts(\"Hello, World!\");

            a = a - 1;
        };
    }",
    );

    let mut parser = ProgramParser::new("./sandbox");
    let program = parser.parse()?;

    println!("Parsed program: {:#?}", program);

    // let mut typer = Typer::new();
    // let code = typer.check(code)?;

    // let mut emitter = Emitter::new(Triple::host())?;
    // let module = emitter.compile(&code)?;

    // let linker = Linker::new("build/som");
    // let executable = linker.link_modules(vec![module])?;

    // let runner = Runner::new(&executable);
    // let result = runner.run()?;

    // println!("Process exited with: {}", result);

    // Ok(())

    todo!();
}
