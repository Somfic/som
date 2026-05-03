use crate::{
    Ast, Expr,
    token::{Token, TokenKind},
};

mod cursor;
mod expr;
mod rules;

pub struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    ast: Ast,
    diags: &'a mut som_common::DiagnosticSink,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Token>, diags: &'a mut som_common::DiagnosticSink) -> Self {
        Self {
            tokens,
            pos: 0,
            ast: Ast::new(),
            diags,
        }
    }

    pub fn parse(mut self) -> Ast {
        while self.peek().kind != TokenKind::Eof {
            self.parse_expr();
        }
        self.ast
    }
}
