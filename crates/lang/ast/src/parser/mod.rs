use crate::{
    Ast, Expr, Stmt,
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
        // program is like within an expression block: statements + optional value expr
        while self.peek().kind != TokenKind::Eof {
            let (stmts, value) = self.parse_inner_block();

            let span = stmts
                .iter()
                .map(|&id| self.ast[id].span())
                .chain(value.map(|id| self.ast[id].span()))
                .reduce(|a, b| a.merge(b))
                .unwrap_or_else(|| self.peek().span);

            let expr = self.ast.add_expr(Expr::Block { stmts, value, span });
            let stmt = self.ast.add_stmt(Stmt::Expr { expr, span });
            self.ast.root.push(stmt);
        }

        self.ast
    }
}
