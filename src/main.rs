use som::{ast::Expression, Diagnostic, Emitter, Parser, Source, Typer};
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
        let fib = fn(n ~ int) -> int {
            n if n < 2 else fib(n - 1) + fib(n - 2)
        };

        fib(18)
    }",
    );

    let mut parser = Parser::new(source);
    let code = parser.parse::<Expression<_>>()?;

    let mut typer = Typer::new();
    let code = typer.check(code)?;

    let mut emitter = Emitter::new(Triple::host());
    let code = emitter.compile(&code)?;

    let result = (unsafe { std::mem::transmute::<*const u8, fn() -> i64>(code) })();

    println!("{:?}", result);

    Ok(())
}
