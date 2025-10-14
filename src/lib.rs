// Library interface for som compiler - used for benchmarking and testing

pub mod compiler;
pub mod errors;
pub mod expressions;
pub mod lexer;
pub mod module;
pub mod parser;
pub mod prelude;
pub mod runner;
pub mod statements;
pub mod type_checker;
pub mod types;

// Re-export commonly used types for convenience
pub use compiler::Compiler;
pub use lexer::Lexer;
pub use parser::Parser;
pub use runner::Runner;
pub use type_checker::TypeChecker;
