use crate::{lexer::Lexer, parser::Parser};

use super::TypeChecker;

#[test]
fn basic_type() {
    let code = r#"
    fn main() {
      let a = 12;
    }
    "#;

    let lexer = Lexer::new(code);
    let mut parser = Parser::new(lexer);
    let symbol = parser.parse().unwrap();

    let typechecker = TypeChecker::new(symbol);
    let errors = typechecker.check();

    assert!(errors.is_empty());
}
