use crate::parser::{BinaryOperation, Expression, Statement, Symbol};

pub struct Transpiler {
    symbol: Symbol,
}

impl Transpiler {
    pub fn new(symbol: Symbol) -> Self {
        Self { symbol }
    }

    pub fn transpile(&self) -> String {
        let mut output = String::new();

        match &self.symbol {
            Symbol::Expression(expression) => {
                output.push_str(&self.transpile_expression(expression));
            }
            Symbol::Statement(statement) => {
                output.push_str(&self.transpile_statement(statement));
            }
            Symbol::Unknown(lexeme) => {
                output.push_str(&format!("Unknown lexeme: {:?}", lexeme));
            }
        }

        output
    }

    #[allow(clippy::only_used_in_recursion)]
    fn transpile_expression(&self, expression: &Expression) -> String {
        match expression {
            Expression::Number(number) => number.to_string(),
            Expression::String(string) => format!("\"{}\"", string),
            Expression::Symbol(symbol) => symbol.clone(),
            Expression::Binary(left, operation, right) => {
                let left = self.transpile_expression(left);
                let operation = match operation {
                    BinaryOperation::Plus => "+",
                    BinaryOperation::Minus => "-",
                    BinaryOperation::Times => "*",
                    BinaryOperation::Divide => "/",
                };
                let right = self.transpile_expression(right);
                format!("{} {} {}", left, operation, right)
            }
            Expression::Grouping(expression) => {
                format!("({})", self.transpile_expression(expression))
            }
        }
    }

    fn transpile_statement(&self, statement: &Statement) -> String {
        match statement {
            Statement::Block(statements) => {
                let mut output = String::new();
                output.push_str("{\n");
                for statement in statements {
                    output.push_str(&format!("  {}", &self.transpile_statement(statement)));
                }
                output.push_str("}\n");
                output
            }
            Statement::Expression(expression) => {
                let expression = self.transpile_expression(expression);
                format!("{};\n", expression)
            }
        }
    }
}
