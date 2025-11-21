use som::{ast::Expression, Diagnostic, Emitter, Linker, Parser, Runner, Source, Typer};
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
        type Color = { r ~ int, g ~ int, b ~ int }; 
        let red = Color { r: 1, g: 1, b: 1 };

        red.r + red.g + red.b
    }",
    );

    let mut parser = Parser::new(source);
    let code = parser.parse::<Expression<_>>()?;

    let mut typer = Typer::new();
    let code = typer.check(code)?;

    let mut emitter = Emitter::new(Triple::host())?;
    let module = emitter.compile(&code)?;

    let linker = Linker::new("build/som");
    let executable = linker.link_modules(vec![module])?;

    let runner = Runner::new(&executable);
    let result = runner.run()?;

    println!("{}", result);

    Ok(())
}
