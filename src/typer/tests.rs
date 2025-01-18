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
    let statements = parser.parse().unwrap();

    let typechecker = TypeChecker::new(&statements);
    let errors = typechecker.check();

    assert!(errors.is_empty());
}
