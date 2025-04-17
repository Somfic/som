mod expression;
mod module;
mod span;
mod statement;
mod typing;
use std::borrow::Cow;

pub use expression::*;
pub use module::*;
pub use span::*;
pub use statement::*;
pub use typing::*;

pub type Identifier<'ast> = Cow<'ast, str>;
