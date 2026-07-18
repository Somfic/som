use som_common::Id;

use crate::{
    Ast, Root, Stmt,
    token::{Token, TokenKind},
};

mod cursor;
mod expr;
mod layout;
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
        // The program is a sequence of items: either a layout node or a
        // statement/expression. Each item's value flows through the same way a
        // block's does — the last one is the program's result.
        loop {
            self.skip_layout();
            if self.peek().kind == TokenKind::Eof {
                break;
            }

            if self.at_layout_item() {
                let layout = self.parse_layout();
                self.ast.root.push(Root::Layout(layout));
            } else {
                let stmt = self.parse_item_stmt();
                self.ast.root.push(Root::Stmt(stmt));
            }
        }

        self.ast
    }

    fn parse_item_stmt(&mut self) -> Id<Stmt> {
        if self.peek().kind == TokenKind::Let {
            let stmt = self.parse_let();
            self.try_eat(TokenKind::Semicolon);
            return stmt;
        }

        let expr = self.parse_expr();
        let span = self.ast[expr].span();
        self.try_eat(TokenKind::Semicolon);
        self.ast.add_stmt(Stmt::Expr { expr, span })
    }
}
