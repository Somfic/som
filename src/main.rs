use som::{ast::Expression, Diagnostic, Emitter, Parser, Source, Typer};
use target_lexicon::Triple;

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
    }
}

fn run() -> Result<(), Diagnostic> {
    let source = Source::from_raw("{ let a = 1; a }");

    let mut parser = Parser::new(source);
    let mut typer = Typer::new();
    let mut emitter = Emitter::new(Triple::host());

    let code = parser.parse::<Expression<_>>()?;
    let code = typer.check(code)?;
    let code = emitter.compile(&code)?;

    let result = (unsafe { std::mem::transmute::<*const u8, fn() -> i64>(code) })();

    println!("{:?}", result);

    Ok(())
}
