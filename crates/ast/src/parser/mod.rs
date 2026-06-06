use crate::{
    Ast, Expr, Stmt,
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
            let expr = self.parse_expr();
            let span = self.ast[expr].span();
            let stmt = self.ast.add_stmt(Stmt::Expr { expr, span });

            self.ast.root.push(stmt);
        }
        self.ast
    }
}
