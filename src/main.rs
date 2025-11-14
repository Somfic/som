use som::{
    ast::{Expression, Pseudo},
    Diagnostic, Parser, Source,
};
use som::{TypeCheck, TypeCheckContext};

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
    }
}

fn run() -> Result<(), Diagnostic> {
    let source = Source::from_raw("1 * 1 + 1 * 1");
    let mut parser = Parser::new(source);
    let untyped: Expression<_> = parser.parse()?;
    println!("{}", untyped.pseudo());

    let typed = untyped.type_check(&mut TypeCheckContext {})?;

    println!("{:?}", typed.0.ty);

    Ok(())
}
