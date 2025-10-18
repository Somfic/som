pub use crate::compiler::Compiler;
pub use crate::expressions::binary::BinaryExpression;
pub use crate::expressions::binary::BinaryOperator;
pub use crate::expressions::conditional::ConditionalExpression;
pub use crate::expressions::field_access::FieldAccessExpression;
pub use crate::expressions::function::Parameter;
pub use crate::expressions::primary::PrimaryExpression;
pub use crate::expressions::struct_constructor::StructConstructorExpression;
pub use crate::expressions::unary::UnaryExpression;
pub use crate::expressions::unary::UnaryOperator;
pub use crate::expressions::ExpressionValue;
pub use crate::expressions::TypedExpressionValue;
pub use crate::lexer::Identifier;
pub use crate::parser::lookup::{BindingPower, Lookup};
pub use crate::runner::Runner;
pub use crate::statements::extern_declaration::ExternDeclarationStatement;
pub use crate::statements::import::ImportStatement;
pub use crate::statements::type_declaration::TypeDeclarationStatement;
pub use crate::statements::variable_declaration::VariableDeclarationStatement;
pub use crate::statements::GenericStatement;
pub use crate::statements::{Statement, StatementValue};
pub use crate::type_checker::TypeChecker;
pub use crate::types::FunctionType;
pub use crate::types::StructType;
pub use crate::types::{Type, TypeValue};
pub use crate::{
    expressions::{Expression, TypedExpression},
    lexer::{Lexer, Token, TokenKind, TokenValue},
};
pub use crate::{parser::Parser, statements::TypedStatement};
pub use cranelift::prelude::types as CompilerType;
pub use cranelift::prelude::{FunctionBuilder, InstBuilder};

// Re-export error types and utilities from the errors module
pub use crate::errors::*;

pub type CompileValue = cranelift::prelude::Value;
pub type CompileEnvironment<'env> = crate::compiler::Environment<'env>;
pub type TypeEnvironment<'env> = crate::type_checker::Environment<'env>;
