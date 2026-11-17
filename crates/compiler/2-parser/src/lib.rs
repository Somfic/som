//! Tokens to syntax tree: recursive descent, a Pratt parser for expressions, and error
//! recovery at expression, statement and declaration level.

mod context;
mod cursor;
mod error;
mod expand;
mod expr;
mod foreign;
mod grammar;
mod item;
mod report;
mod stmt;
mod ty;

pub use error::ParseError;
pub use expand::expand_reflected;

use som_ast::{Ast, Item, Visibility};
use som_base::Source;
use som_lexer::{Token, lex};
use std::path::PathBuf;
use std::sync::Arc;

use context::{Context, MemberConditions, PendingWheres};

/// Parses a single source as the root module of a new program.
pub fn parse(source: Arc<Source>) -> (Ast, Vec<ParseError>) {
    let mut ast = Ast::default();
    let root = ast.add_module("", PathBuf::new());
    let errors = parse_file(source, &mut ast, root);
    (ast, errors)
}

/// Parses one source file, adding its items to an existing module of `ast`.
pub fn parse_file(source: Arc<Source>, ast: &mut Ast, module: usize) -> Vec<ParseError> {
    ast.modules[module].files.push(source.clone());
    let mut parser = Parser::new(lex(source), ast, module);
    parser.report_foreign_comments();
    parser.skip_trivia();
    parser.parse_program();
    parser.errors
}

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    ast: &'a mut Ast,
    module: usize,
    errors: Vec<ParseError>,
    /// After an error, until a token is taken as expected: what's wrong meanwhile is likely
    /// the first error's echo, so it isn't reported.
    in_recovery: bool,
    ctx: Context,
    /// The `pub` or `mod` just read, for the item, method or field after it to take.
    visibility: Visibility,
    wheres: PendingWheres,
    when: MemberConditions,
    /// The traits the trait being read builds on, which its methods' `Self` implements.
    trait_supers: Vec<som_ast::Path>,
}

impl<'a> Parser<'a> {
    /// `tokens` end in an `Eof`.
    fn new(tokens: Vec<Token>, ast: &'a mut Ast, module: usize) -> Self {
        Parser {
            tokens,
            pos: 0,
            ast,
            module,
            errors: Vec::new(),
            in_recovery: false,
            ctx: Context::default(),
            visibility: Visibility::Private,
            wheres: PendingWheres::default(),
            when: MemberConditions::default(),
            trait_supers: Vec::new(),
        }
    }

    fn add_item(&mut self, item: Item) {
        self.ast.modules[self.module].items.push(item);
    }
}

/// Something that could be either a statement or a trailing (value) expression.
enum StmtOrExpr {
    Stmt(som_ast::Id<som_ast::Stmt>),
    Expr(som_ast::Id<som_ast::Expr>),
    Error,
}

/// Decimal, `0x` hexadecimal or `0b` binary, with `_` between digits ignored. `None` if it
/// doesn't fit in 128 bits.
pub(crate) fn int_value(text: &str) -> Option<u128> {
    let (digits, radix) = match text.get(..2) {
        Some("0x") => (&text[2..], 16),
        Some("0b") => (&text[2..], 2),
        _ => (text, 10),
    };
    let digits: String = digits.chars().filter(|&c| c != '_').collect();
    u128::from_str_radix(&digits, radix).ok()
}
