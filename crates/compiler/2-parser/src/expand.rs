//! `@for` over items: `@for T in [u8, u16] { impl Hash for T { … } }`. Each copy of the body
//! is the items with `T` written as that element, before anything else sees them.
//!
//! It's done on tokens, before parsing them: the tokens of a copy of the body are put where
//! the `@for` was, then parsed like anything else. Each copy's tokens keep where they're
//! written, so what's wrong in one is said where the body is.

use som_ast::{
    Arm, Ast, Expr, Id, Ident, Path, PerType, Reflected, ReflectedInto, Stmt, Struct, Variant,
    VariantKind,
};
use som_base::message::code;
use som_base::{Source, Span, msg};
use som_lang::Builtin;
use som_lexer::{Token, TokenKind, lex};
use std::sync::Arc;

use crate::report::RecoveryLevel;
use crate::{ParseError, Parser, StmtOrExpr};

/// What can start an item: an `@for` whose body starts with one is over items.
const ITEM_START: &[TokenKind] = &[
    TokenKind::Fn,
    TokenKind::Impl,
    TokenKind::Struct,
    TokenKind::Enum,
    TokenKind::Trait,
    TokenKind::Type,
    TokenKind::Pub,
    TokenKind::Mod,
    TokenKind::Extern,
    TokenKind::Use,
    TokenKind::Builtin,
    TokenKind::Prop,
];

impl Parser<'_> {
    /// At an `@for` whose body is items, not statements: `@for T in [u8, u16] { impl … }`.
    /// In an impl block, `members`, it's methods, or anything else a block has.
    pub(crate) fn at_item_for(&self, members: bool) -> bool {
        if !self.at_builtin(Builtin::For) {
            return false;
        }
        if members {
            return true;
        }
        // Past `@for name in`, then what it loops over, to the body's `{`.
        let mut n = 3;
        let mut depth = 0;
        loop {
            match self.peek_nth(n) {
                TokenKind::OpenBracket | TokenKind::OpenParen => depth += 1,
                TokenKind::CloseBracket | TokenKind::CloseParen => depth -= 1,
                TokenKind::OpenBrace if depth == 0 => break,
                TokenKind::Eof | TokenKind::Semicolon => return false,
                _ => {}
            }
            n += 1;
        }
        ITEM_START.contains(&self.peek_nth(n + 1))
    }

    /// Writes the `@for` here out: a copy of its body's tokens for each element, in its place.
    /// What's after it is then parsed as if they'd been written.
    pub(crate) fn expand_for(&mut self) {
        let start = self.pos;
        let keyword = self.current_span();
        self.advance();
        let Some((name, elements)) = self.parse_for_header(keyword) else {
            self.skip_for(start);
            return;
        };
        if self
            .expect(TokenKind::OpenBrace, "`{` and the items")
            .is_none()
        {
            self.skip_for(start);
            return;
        }
        // The body's tokens, trivia and all, up to its `}`.
        let body_start = self.pos;
        let end = self.closing_brace(body_start);
        let body = self.tokens[body_start..end].to_vec();
        let past = (end + 1).min(self.tokens.len() - 1);
        let mut written = Vec::new();
        for element in &elements {
            for token in &body {
                match token.kind == TokenKind::Ident && *token.text == *name.name {
                    true => written.extend(element.iter().cloned()),
                    false => written.push(token.clone()),
                }
            }
        }
        self.tokens.splice(start..past, written);
        self.pos = start;
        self.skip_trivia();
    }

    /// `name in [a, b]` or `name in 0..4`, after `@for`: the name, and each element's tokens.
    fn parse_for_header(&mut self, keyword: Span) -> Option<(Ident, Vec<Vec<Token>>)> {
        let name = self.parse_ident("a name for each element")?;
        self.expect(TokenKind::In, "`in` and what to loop over")?;
        if self.at(TokenKind::OpenBracket) {
            return Some((name, self.parse_for_elements()));
        }
        if let Some(range) = self.int_range() {
            return Some((name, range));
        }
        self.error_at(
            keyword,
            msg![
                "an ",
                code("@for"),
                " around items goes through a list written out, or a range of numbers"
            ],
            msg![
                "write it ",
                code("@for T in [u8, u16] { … }"),
                " or ",
                code("@for i in 0..4 { … }")
            ],
        );
        None
    }

    /// `[a, b]`, at the `[`: each element's tokens.
    fn parse_for_elements(&mut self) -> Vec<Vec<Token>> {
        self.advance();
        let mut elements = vec![Vec::new()];
        let mut depth = 0;
        while !self.at_eof() {
            let token = self.peek_token().clone();
            match token.kind {
                TokenKind::CloseBracket if depth == 0 => break,
                TokenKind::Comma if depth == 0 => {
                    elements.push(Vec::new());
                    self.advance();
                    continue;
                }
                TokenKind::OpenBracket
                | TokenKind::OpenParen
                | TokenKind::OpenBrace
                | TokenKind::LessThan => depth += 1,
                TokenKind::CloseBracket
                | TokenKind::CloseParen
                | TokenKind::CloseBrace
                | TokenKind::GreaterThan => depth -= 1,
                _ => {}
            }
            elements.last_mut().expect("one at least").push(token);
            self.advance();
        }
        self.expect(TokenKind::CloseBracket, "`]` after the elements");
        // `[a, b,]`: a comma after the last.
        elements.into_iter().filter(|e| !e.is_empty()).collect()
    }

    /// The index of the `}` closing a body that starts at `start`, inside its `{`; or of the
    /// `Eof`, if none does.
    fn closing_brace(&self, start: usize) -> usize {
        let mut depth = 1;
        let mut end = start;
        while end < self.tokens.len() && self.tokens[end].kind != TokenKind::Eof {
            match self.tokens[end].kind {
                TokenKind::OpenBrace => depth += 1,
                TokenKind::CloseBrace => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
        end
    }

    /// At `@for v in Kind` or `@for (i, v) in Kind`: over a type, its variants, written out
    /// once every module is loaded. A list `[…]` or a range of numbers is written out at once.
    pub(crate) fn at_reflected_for(&self) -> bool {
        if !self.at_builtin(Builtin::For) {
            return false;
        }
        let mut n = 1;
        while !matches!(
            self.peek_nth(n),
            TokenKind::In | TokenKind::Eof | TokenKind::OpenBrace
        ) {
            n += 1;
        }
        // A built-in there, `@variants(Kind)`, is read here too, to be said to be unknown.
        self.peek_nth(n) == TokenKind::In
            && (self.peek_nth(n + 1) == TokenKind::Builtin
                || self.peek_nth(n + 1) == TokenKind::Ident
                    && self.peek_nth(n + 2) != TokenKind::DotDot)
    }

    /// `@for (i, v) in Kind { … }`: what each copy calls the element, the type, and the
    /// body, kept to be written out once `Kind` is known.
    pub(crate) fn parse_reflected_for(&mut self) -> Option<(Vec<Ident>, Path, Span)> {
        self.advance();
        let mut names = Vec::new();
        if self.eat(TokenKind::OpenParen) {
            loop {
                names.push(self.parse_ident("a name")?);
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::CloseParen, "`)` after the names")?;
        } else {
            names.push(self.parse_ident("a name for each element")?);
        }
        self.expect(TokenKind::In, "`in` and what to loop over")?;
        // `@variants(Kind)`: there's no such built-in. The rest of the `@for` is skipped.
        if self.at(TokenKind::Builtin) {
            let name = self.peek_token().text.to_string();
            self.error_unknown_builtin(self.current_span(), &name);
            self.skip_to_braces();
            return None;
        }
        let of = self.parse_path("a type")?;
        let open = self.current_span();
        self.expect(TokenKind::OpenBrace, "`{` and what to write for each")?;
        // To the matching `}`: the body is what's between.
        let body_start = open.start_offset + 1;
        let mut depth = 1;
        let mut body_end = body_start;
        while !self.at_eof() {
            match self.peek() {
                TokenKind::OpenBrace => depth += 1,
                TokenKind::CloseBrace => {
                    depth -= 1;
                    if depth == 0 {
                        body_end = self.current_span().start_offset;
                        self.advance();
                        break;
                    }
                }
                _ => {}
            }
            self.advance();
        }
        let body = Span::from_range(body_start..body_end, open.source.clone());
        Some((names, of, body))
    }

    /// `0..4`, of literals: each number, as a token.
    fn int_range(&mut self) -> Option<Vec<Vec<Token>>> {
        if self.peek_nth(0) != TokenKind::Int
            || self.peek_nth(1) != TokenKind::DotDot
            || self.peek_nth(2) != TokenKind::Int
        {
            return None;
        }
        let first = self.peek_token().clone();
        self.advance_by(2);
        let last = self.peek_token().clone();
        self.advance();
        let number = |t: &Token| t.text.replace('_', "").parse::<i64>().ok();
        let (from, to) = (number(&first)?, number(&last)?);
        let token = |n: i64| Token {
            kind: TokenKind::Int,
            text: n.to_string().into(),
            span: first.span.clone(),
        };
        Some((from..to).map(|n| vec![token(n)]).collect())
    }

    /// Past an `@for` that couldn't be read, from `start`: to the end of its body.
    fn skip_for(&mut self, start: usize) {
        if self.pos == start {
            self.advance();
        }
        self.skip_to_braces();
    }
}

/// Writes out each `@for` over a type, once every module is loaded: `@for (i, v) in Kind
/// { v => i, }` is an arm for each of `Kind`'s variants, `v` matching it and `i` its index.
pub fn expand_reflected(ast: &mut Ast) -> Vec<ParseError> {
    let mut errors = Vec::new();
    let mut reflected = std::mem::take(&mut ast.reflected);
    // Later positions first, so earlier ones stay where they are.
    reflected.sort_by_key(|r| match r.into {
        ReflectedInto::Arms { expr, at } => std::cmp::Reverse((expr.id, at)),
        ReflectedInto::Stmts { stmt } => std::cmp::Reverse((usize::MAX, stmt.id)),
    });
    for r in reflected {
        let found = reflected_type(ast, &r);
        let generic = found.is_none() && r.of.segments.len() == 1;
        match (found, &r.into) {
            (_, &ReflectedInto::Arms { expr, at }) if generic => {
                expand_per_type_match(ast, &r, expr, at, &mut errors)
            }
            (_, &ReflectedInto::Stmts { stmt }) if generic => {
                expand_per_type_stmts(ast, &r, stmt, &mut errors)
            }
            (None, _) => errors.push(ParseError::new(
                r.of.span(),
                msg![
                    "there's no enum or struct ",
                    code(r.of.name().name.to_string()),
                    " to go through"
                ],
                msg![
                    code("@for v in Kind"),
                    " goes through an enum's variants, ",
                    code("@for f in Point"),
                    " a struct's fields; not yet a generic type's"
                ],
            )),
            (Some(s), into) => {
                let written = match write_copies(ast, &r, s) {
                    Ok(written) => written,
                    Err(error) => {
                        errors.push(error);
                        continue;
                    }
                };
                match *into {
                    ReflectedInto::Arms { expr, at } => {
                        let (arms, arm_errors) = parse_arms(ast, r.module, written);
                        errors.extend(arm_errors);
                        if let Expr::Match { arms: existing, .. } = ast.exprs.get_mut(expr) {
                            existing.splice(at..at, arms);
                        }
                    }
                    ReflectedInto::Stmts { stmt } => {
                        let (block, block_errors) =
                            parse_block(ast, r.module, written, r.body.clone());
                        *ast.stmts.get_mut(stmt) = Stmt::Expr(block);
                        errors.extend(block_errors);
                    }
                }
            }
        }
    }
    errors
}

/// The enum or struct `r` goes through: in the module it's written in, or else the only one
/// by that name.
fn reflected_type(ast: &Ast, r: &Reflected) -> Option<Id<Struct>> {
    let name = &r.of.name().name;
    let found: Vec<(usize, Id<Struct>)> = (ast.modules.iter().enumerate())
        .flat_map(|(m, module)| module.items.iter().map(move |item| (m, item)))
        .filter_map(|(m, item)| match item {
            som_ast::Item::Struct(s) if ast.structs[*s].name.name == *name => Some((m, *s)),
            _ => None,
        })
        .collect();
    (found.iter().find(|(m, _)| *m == r.module))
        .or_else(|| (found.len() == 1).then(|| &found[0]))
        .map(|&(_, s)| s)
}

/// Among a `match`'s arms, over a generic `T`: the `match` written out for every enum, with
/// that enum's arms, each instance picking its copy.
fn expand_per_type_match(
    ast: &mut Ast,
    r: &Reflected,
    expr: Id<Expr>,
    at: usize,
    errors: &mut Vec<ParseError>,
) {
    if ast.per_type_matches.contains_key(&expr) {
        errors.push(ParseError::new(
            r.of.span(),
            msg![
                "a ",
                code("match"),
                " has one ",
                code("@for"),
                " over a generic type among its arms, for now"
            ],
            msg!["write the other arms out, or go through the type once"],
        ));
        return;
    }
    let enums: Vec<Id<Struct>> = (ast.structs.iter())
        .filter(|(_, s)| s.is_enum())
        .map(|(id, _)| id)
        .collect();
    let mut copies = Vec::new();
    for (k, e) in enums.into_iter().enumerate() {
        let written = match write_copies(ast, r, e) {
            Ok(written) => written,
            Err(error) => {
                errors.push(error);
                break;
            }
        };
        let (new_arms, arm_errors) = parse_arms(ast, r.module, written);
        // Every copy is the same text: what's wrong is said for the first.
        if k == 0 {
            errors.extend(arm_errors);
        }
        let Expr::Match { scrutinee, arms } = &ast.exprs[expr] else {
            break;
        };
        let scrutinee = *scrutinee;
        let mut all: Vec<Arm> = (arms.iter())
            .map(|a| Arm {
                pattern: a.pattern,
                body: a.body,
            })
            .collect();
        all.splice(at.min(all.len())..at.min(all.len()), new_arms);
        let span = ast.exprs.span(expr).clone();
        let copy = Expr::Match {
            scrutinee,
            arms: all,
        };
        copies.push((e, ast.exprs.alloc(copy, span)));
    }
    let per_type = PerType {
        of: r.of.clone(),
        arms: copies,
    };
    ast.per_type_matches.insert(expr, per_type);
}

/// Not a type there is: in a function, a generic one's, `T`. Written out for every struct,
/// the function's instances each picking theirs (whether `T` is one is the type checker's to
/// say).
fn expand_per_type_stmts(
    ast: &mut Ast,
    r: &Reflected,
    stmt: Id<Stmt>,
    errors: &mut Vec<ParseError>,
) {
    let structs: Vec<Id<Struct>> = (ast.structs.iter())
        .filter(|(_, s)| !s.is_enum())
        .map(|(id, _)| id)
        .collect();
    let mut arms = Vec::new();
    for (k, s) in structs.into_iter().enumerate() {
        let written = match write_copies(ast, r, s) {
            Ok(written) => written,
            Err(error) => {
                errors.push(error);
                break;
            }
        };
        let (block, copy_errors) = parse_block(ast, r.module, written, r.body.clone());
        // Every copy is the same text: what's wrong is said for the first.
        if k == 0 {
            errors.extend(copy_errors);
        }
        arms.push((s, block));
    }
    let per_type = PerType {
        of: r.of.clone(),
        arms,
    };
    ast.per_type.insert(stmt, per_type);
}

/// `tokens` parsed as a `match`'s arms.
fn parse_arms(ast: &mut Ast, module: usize, tokens: Vec<Token>) -> (Vec<Arm>, Vec<ParseError>) {
    let mut parser = Parser::new(tokens, ast, module);
    parser.skip_trivia();
    let mut arms = Vec::new();
    while !parser.at_eof() {
        let Some(arm) = parser.parse_arm() else {
            break;
        };
        let more = parser.arm_continues(&arm);
        arms.push(arm);
        if !more {
            break;
        }
    }
    (arms, parser.errors)
}

/// `tokens` parsed as statements: a block of them.
fn parse_block(
    ast: &mut Ast,
    module: usize,
    tokens: Vec<Token>,
    span: Span,
) -> (Id<Expr>, Vec<ParseError>) {
    let mut parser = Parser::new(tokens, ast, module);
    parser.skip_trivia();
    let mut stmts = Vec::new();
    while !parser.at_eof() {
        let before = parser.pos;
        match parser.parse_stmt_or_expr() {
            StmtOrExpr::Stmt(s) => stmts.push(s),
            StmtOrExpr::Expr(e) => {
                stmts.push(parser.expr_stmt(e));
                parser.eat(TokenKind::Semicolon);
            }
            StmtOrExpr::Error => parser.recover(RecoveryLevel::Statement),
        }
        if parser.pos == before {
            parser.advance();
        }
    }
    let errors = parser.errors;
    let block = ast.exprs.alloc(Expr::Block { stmts, value: None }, span);
    (block, errors)
}

/// What each copy writes for the element: a variant's pattern, `Circle(_)`, or a field's
/// name.
fn element_texts(s: &Struct) -> Vec<String> {
    match s.is_enum() {
        true => s.variants().iter().map(variant_pattern).collect(),
        false => (s.fields.iter()).map(|f| f.name.name.to_string()).collect(),
    }
}

fn variant_pattern(variant: &Variant) -> String {
    let name = &variant.name.name;
    match variant.kind {
        VariantKind::Unit => name.to_string(),
        VariantKind::Tuple => {
            let blanks = vec!["_"; variant.fields.len()];
            format!("{name}({})", blanks.join(", "))
        }
        VariantKind::Named => format!("{name} {{ .. }}"),
    }
}

/// The tokens of `r`'s copies for enum or struct `s`: its body once for each variant or
/// field, the element written as it (and its index, if it's named), then an `Eof`.
fn write_copies(ast: &Ast, r: &Reflected, s: Id<Struct>) -> Result<Vec<Token>, ParseError> {
    let elements = element_texts(&ast.structs[s]);
    let body: Vec<Token> = (lex(r.body.source.clone()).into_iter())
        .filter(|t| {
            t.kind != TokenKind::Eof
                && t.span.start_offset >= r.body.start_offset
                && t.span.start_offset < r.body.start_offset + r.body.length
        })
        .collect();
    let (index, element) = match &r.names[..] {
        [element] => (None, element),
        [index, element] => (Some(index), element),
        _ => {
            return Err(ParseError::new(
                r.names[0].span.clone(),
                msg!["an ", code("@for"), " over a type names one or two things"],
                msg![code("v"), ", or its index and it: ", code("(i, v)")],
            ));
        }
    };
    let mut written = Vec::new();
    for (i, pattern) in elements.iter().enumerate() {
        for token in &body {
            let is = |name: &Ident| token.kind == TokenKind::Ident && *token.text == *name.name;
            if index.is_some_and(is) {
                written.push(Token {
                    kind: TokenKind::Int,
                    text: i.to_string().into(),
                    span: token.span.clone(),
                });
            } else if is(element) {
                // Said to be where `v` is.
                let pattern = lex(Arc::new(Source::from_raw(pattern.as_str())));
                written.extend(
                    (pattern.into_iter())
                        .filter(|t| t.kind != TokenKind::Eof)
                        .map(|t| Token {
                            span: token.span.clone(),
                            ..t
                        }),
                );
            } else {
                written.push(token.clone());
            }
        }
    }
    written.push(Token {
        kind: TokenKind::Eof,
        text: "".into(),
        span: r.body.clone(),
    });
    Ok(written)
}
