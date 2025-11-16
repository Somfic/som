use som::{ast::Expression, Diagnostic, Emitter, Parser, Source, Typer, Untyped};
use target_lexicon::Triple;

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
    }
}

fn run() -> Result<(), Diagnostic> {
    let source = Source::from_raw("1 + 1");

    let mut parser = Parser::new(source);
    let mut typer = Typer::new();
    let mut emitter = Emitter::new(Triple::host());

    let expression = parser.parse::<Expression<Untyped>>()?;
    let expression = typer.check(expression)?;
    let code = emitter.compile(&expression)?;

    let result = (unsafe { std::mem::transmute::<*const u8, fn() -> i64>(code) })();

    println!("{:?}", result);

    Ok(())
}
