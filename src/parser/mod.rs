use crate::lexer::{lex, Syntax};
use rowan::{GreenNode, GreenNodeBuilder};
use std::collections::HashSet;

mod error;
pub use error::*;

pub mod cst;

mod dec;
mod expr;
mod stmt;
mod ty;

pub struct Input<'a> {
    tokens: Vec<(Syntax, &'a str, usize)>, // (syntax, text, byte_offset)
    pos: usize,
}

impl<'a> Input<'a> {
    pub fn new(_source: &'a str, tokens: Vec<(Syntax, &'a str)>) -> Self {
        // Calculate byte offsets for each token
        let mut offset = 0;
        let tokens_with_pos: Vec<_> = tokens
            .into_iter()
            .map(|(syntax, text)| {
                let start = offset;
                offset += text.len();
                (syntax, text, start)
            })
            .collect();

        Self {
            tokens: tokens_with_pos,
            pos: 0,
        }
    }

    pub fn peek(&self) -> Syntax {
        self.tokens
            .get(self.pos)
            .map(|(syntax, _, _)| *syntax)
            .unwrap_or(Syntax::EndOfFile)
    }

    pub fn current_pos(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map(|(_, _, pos)| *pos)
            .unwrap_or(0)
    }

    pub fn advance(&mut self) -> (Syntax, &'a str) {
        if let Some((syntax, text, _)) = self.tokens.get(self.pos) {
            self.pos += 1;
            (*syntax, *text)
        } else {
            (Syntax::EndOfFile, "")
        }
    }

    pub fn at(&self, syntax: Syntax) -> bool {
        self.peek() == syntax
    }

    pub fn at_any(&self, anchor: &HashSet<Syntax>) -> bool {
        anchor.contains(&self.peek())
    }
}

pub struct Parser<'a> {
    input: Input<'a>,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
    in_error: bool,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        let tokens = lex(source);
        Self {
            input: Input::new(source, tokens),
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
            in_error: false,
        }
    }

    pub fn parse(mut self) -> (GreenNode, Vec<ParseError>) {
        self.program();
        (self.builder.finish(), self.errors)
    }

    fn with<F>(&mut self, syntax: Syntax, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.builder.start_node(syntax.into());
        f(self);
        self.builder.finish_node();
    }

    fn eat(&mut self, syntax: Syntax) -> Option<&'a str> {
        if self.input.at(syntax) {
            let (_, text) = self.input.advance();
            self.builder.token(syntax.into(), text);

            // Consume whitespace and comments
            while matches!(self.input.peek(), Syntax::Whitespace | Syntax::Comment) {
                let (ws_syntax, ws_text) = self.input.advance();
                self.builder.token(ws_syntax.into(), ws_text);
            }

            self.in_error = false;
            Some(text)
        } else {
            None
        }
    }

    fn expect(&mut self, syntax: Syntax, anchor: HashSet<Syntax>) {
        if self.eat(syntax).is_none() {
            self.recover_until(anchor, vec![syntax]);
        }
    }

    fn bump(&mut self) {
        let (syntax, text) = self.input.advance();
        self.builder.token(syntax.into(), text);

        // Consume whitespace and comments
        while matches!(self.input.peek(), Syntax::Whitespace | Syntax::Comment) {
            let (ws_syntax, ws_text) = self.input.advance();
            self.builder.token(ws_syntax.into(), ws_text);
        }

        self.in_error = false;
    }

    fn recover_until(&mut self, anchor: HashSet<Syntax>, expected: Vec<Syntax>) {
        if !self.in_error {
            let pos = self.input.current_pos();
            let span = crate::span::Span::new(pos as u32, pos as u32);
            self.errors
                .push(ParseError::new(expected, self.input.peek(), span));
            self.in_error = true;
        }

        self.builder.start_node(Syntax::Error.into());

        while !self.input.at_any(&anchor) && !self.input.at(Syntax::EndOfFile) {
            let (syntax, text) = self.input.advance();
            self.builder.token(syntax.into(), text);
        }

        self.builder.finish_node();
    }
}

pub(crate) fn union<const N: usize>(base: &HashSet<Syntax>, items: [Syntax; N]) -> HashSet<Syntax> {
    let mut result = base.clone();
    result.extend(items);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_function() {
        let input = "fn add(x: i32, y: i32) -> i32 { x + y }";
        let parser = Parser::new(input);
        let (_tree, errors) = parser.parse();
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_function_with_let() {
        let input = "fn test() { let x: i32 = 5; x }";
        let parser = Parser::new(input);
        let (_tree, errors) = parser.parse();
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_binary_expr() {
        let input = "fn test() { 1 + 2 * 3 }";
        let parser = Parser::new(input);
        let (_tree, errors) = parser.parse();
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_function_call() {
        let input = "fn test() { add(1, 2) }";
        let parser = Parser::new(input);
        let (_tree, errors) = parser.parse();
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }
}
