use som_ast::{Extern, Func, Id, Span};
use som_base::message::code;
use som_base::msg;
use som_lexer::TokenKind;

use super::signature;
use crate::Parser;
use crate::cursor::unquoted;
use crate::report::RecoveryLevel;

/// `extern "raylib" from "raylib.h"`: only a keyword there, so it can still name things
/// elsewhere.
const FROM: &str = "from";
const HEADER_EXTENSION: &str = ".h";

impl Parser<'_> {
    /// `use extern "raylib" …`: an extern block whose library module is also `use`d.
    pub(super) fn parse_use_extern(&mut self) -> Option<Extern> {
        self.expect(TokenKind::Use, "a use statement")?;
        let mut block = self.parse_extern()?;
        if block.library.is_none() {
            let span = self.previous_span();
            self.error_at(
                span,
                msg!["only a library's declarations can be ", code("use"), "d"],
                msg!["name the library: ", code("use extern \"lib\" { … }")],
            );
        }
        block.use_module = true;
        Some(block)
    }

    /// `extern "lib" { fn foo(); }`, or `extern "lib" from "lib.h";`.
    pub(super) fn parse_extern(&mut self) -> Option<Extern> {
        let start = self.current_span();
        self.expect(TokenKind::Extern, "an extern block")?;
        let library = self.at(TokenKind::Text).then(|| self.parse_quoted());
        let header = match self.ident_ahead(0) == Some(FROM) {
            true => Some(self.parse_header()?),
            false => None,
        };
        // `extern "raylib";` is short for `extern "raylib" from "raylib.h";`.
        if (header.is_some() || library.is_some()) && self.eat(TokenKind::Semicolon) {
            let header = header.or_else(|| {
                let (name, span) = library.clone().expect("a library or a header");
                Some((format!("{name}{HEADER_EXTENSION}"), span))
            });
            return Some(self.extern_block(start, library, header, Vec::new()));
        }
        self.expect(
            TokenKind::OpenBrace,
            "a list of extern function declarations",
        )?;
        let mut funcs = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            self.visibility = self.parse_visibility();
            if !self.at_fn() {
                self.error_missing(
                    "expected extern function declaration",
                    "an extern function declaration",
                );
                self.recover(RecoveryLevel::Declaration);
                continue;
            }
            match self.parse_extern_func() {
                Some(id) => funcs.push(id),
                None => self.recover(RecoveryLevel::Declaration),
            }
        }
        self.expect(TokenKind::CloseBrace, "a closing brace after extern block")?;
        if library.as_ref().is_some_and(|(l, _)| l == som_lang::GPU_BUILTINS_LIBRARY) {
            self.ast.gpu_builtins.extend(funcs.iter().copied());
        }
        Some(self.extern_block(start, library, header, funcs))
    }

    fn extern_block(
        &self,
        start: Span,
        library: Option<(String, Span)>,
        header: Option<(String, Span)>,
        funcs: Vec<Id<Func>>,
    ) -> Extern {
        let (library, library_span) = library.unzip();
        Extern {
            span: self.span_from(&start),
            library,
            library_span,
            header,
            use_module: false,
            module: None,
            funcs,
        }
    }

    /// A text literal's text, and where it is.
    fn parse_quoted(&mut self) -> (String, Span) {
        let token = self.peek_token();
        let quoted = (unquoted(&token.text).to_string(), token.span.clone());
        self.advance();
        quoted
    }

    /// `from "raylib.h"`, at `from`: a C header to import the declarations from.
    fn parse_header(&mut self) -> Option<(String, Span)> {
        self.advance();
        if !self.at(TokenKind::Text) {
            self.error_expected(&[TokenKind::Text], "a header file name");
            return None;
        }
        Some(self.parse_quoted())
    }

    /// `fn name(params) -> Ret;`
    fn parse_extern_func(&mut self) -> Option<Id<Func>> {
        let doc = self.doc_comment();
        let visibility = std::mem::take(&mut self.visibility);
        let start = self.current_span();
        self.expect_fn("a function declaration")?;
        let name = self.parse_ident("a function name")?;
        let (params, variadic) = self.parse_params(true)?;
        let returns = if self.void_return() {
            None
        } else if self.eat(TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(
            TokenKind::Semicolon,
            "a semicolon after extern function declaration",
        )?;
        let span = self.span_from(&start);
        let func = Func {
            params,
            returns,
            variadic,
            ..signature(doc, visibility, name)
        };
        Some(self.ast.funcs.alloc(func, span))
    }
}
