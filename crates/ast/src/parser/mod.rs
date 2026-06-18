use crate::{
    Ast, Stmt,
    token::{Token, TokenKind},
};

mod cursor;
mod expr;
mod rules;
mod stmt;

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
            let stmt = self.parse_stmt();
            self.ast.root.push(stmt);
        }
        self.ast
    }
}
