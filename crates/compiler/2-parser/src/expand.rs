//! `@for` over items: `@for T in [u8, u16] { impl Hash for T { … } }`. Each copy of the body
//! is the items with `T` written as that element, before anything else sees them: resolve and
//! the type checker see one `impl` for each type, written out.
//!
//! It's done on tokens, before parsing them: the tokens of a copy of the body are put where
//! the `@for` was, then parsed like anything else. Each copy's tokens keep where they're
//! written, so what's wrong in one is said where the body is.

use som_ast::{Ast, Expr, Ident, Path, ReflectedInto, VariantKind};
use som_base::message::code;
use som_base::{Source, Span, msg};
use som_lexer::{Token, TokenKind, lex};

use crate::{ParseError, Parser};

impl Parser<'_> {
    /// At an `@for` whose body is items, not statements: `@for T in [u8, u16] { impl … }`.
    /// In an impl block, `members`, it's methods, or anything else a block has.
    pub(crate) fn at_item_for(&self, members: bool) -> bool {
        if !(self.at(TokenKind::Builtin) && &*self.peek_token().text == "@for") {
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
        self.peek_nth(n) == TokenKind::OpenBrace
            && matches!(
                self.peek_nth(n + 1),
                TokenKind::Fn
                    | TokenKind::Impl
                    | TokenKind::Struct
                    | TokenKind::Enum
                    | TokenKind::Trait
                    | TokenKind::Type
                    | TokenKind::Pub
                    | TokenKind::Mod
                    | TokenKind::Extern
                    | TokenKind::Use
                    | TokenKind::Builtin
                    | TokenKind::Prop
            )
    }

    /// Writes the `@for` here out: a copy of its body's tokens for each element, in its place.
    /// What's after it is then parsed as if they'd been written.
    pub(crate) fn expand_for(&mut self) {
        let start = self.pos;
        let keyword = self.current_span();
        self.advance();
        let Some(name) = self.parse_ident("a name for each element") else {
            self.skip_for(start);
            return;
        };
        if self
            .expect(TokenKind::In, "`in` and what to loop over")
            .is_none()
        {
            self.skip_for(start);
            return;
        }
        // Each element's tokens.
        let elements: Vec<Vec<Token>> = if self.at(TokenKind::OpenBracket) {
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
        } else if let Some(range) = self.int_range() {
            range
        } else {
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
        let mut depth = 1;
        let mut end = body_start;
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

    /// At `@for v in Kind` or `@for (i, v) in Kind`: over a type, its variants, written out
    /// once every module is loaded. A list `[…]` or a range of numbers is written out at once.
    pub(crate) fn at_reflected_for(&self) -> bool {
        if !(self.at(TokenKind::Builtin) && &*self.peek_token().text == "@for") {
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
            let known: Vec<&str> = som_lexer::BUILTINS.iter().map(|b| b.0).collect();
            let name = self.peek_token().text.to_string();
            self.error_at(
                self.current_span(),
                msg!["unknown built-in ", code(name)],
                msg!["the built-ins are ", code(known.join(", "))],
            );
            while !self.at_eof() && !self.at(TokenKind::OpenBrace) {
                self.advance();
            }
            if self.at(TokenKind::OpenBrace) {
                self.skip_balanced(TokenKind::OpenBrace, TokenKind::CloseBrace);
            }
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
        self.advance();
        self.advance();
        let last = self.peek_token().clone();
        self.advance();
        let number = |t: &Token| t.text.replace('_', "").parse::<i64>().ok();
        let (from, to) = (number(&first)?, number(&last)?);
        let span = first.span.clone();
        Some(
            (from..to)
                .map(|n| {
                    vec![Token {
                        kind: TokenKind::Int,
                        text: n.to_string().into(),
                        span: span.clone(),
                    }]
                })
                .collect(),
        )
    }

    /// Past an `@for` that couldn't be read, from `start`: to the end of its body.
    fn skip_for(&mut self, start: usize) {
        if self.pos == start {
            self.advance();
        }
        while !self.at_eof() && !self.at(TokenKind::OpenBrace) {
            self.advance();
        }
        if self.at(TokenKind::OpenBrace) {
            self.skip_balanced(TokenKind::OpenBrace, TokenKind::CloseBrace);
        }
    }
}

/// What a variant is, to match it: `Red`, `Circle(_)`, `Rect { .. }`.
fn variant_pattern(variant: &som_ast::Variant) -> String {
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
        // The enum or struct: in the module it's written in, or else the only one by that
        // name.
        let name = &r.of.name().name;
        let enums: Vec<(usize, som_ast::Id<som_ast::Struct>)> = (ast.modules.iter().enumerate())
            .flat_map(|(m, module)| module.items.iter().map(move |item| (m, item)))
            .filter_map(|(m, item)| match item {
                som_ast::Item::Struct(s) if &*ast.structs[*s].name.name == &**name => Some((m, *s)),
                _ => None,
            })
            .collect();
        let found = (enums.iter().find(|(m, _)| *m == r.module))
            .or_else(|| (enums.len() == 1).then(|| &enums[0]))
            .map(|&(_, s)| s);
        // Among a `match`'s arms, over a generic `T`: the `match` written out for every enum,
        // with that enum's arms, each instance picking its copy.
        if found.is_none()
            && r.of.segments.len() == 1
            && let ReflectedInto::Arms { expr, at } = r.into
        {
            if ast.per_type_matches.contains_key(&expr) {
                errors.push(ParseError {
                    message: msg![
                        "a ",
                        code("match"),
                        " has one ",
                        code("@for"),
                        " over a generic type among its arms, for now"
                    ],
                    hint: msg!["write the other arms out, or go through the type once"],
                    span: r.of.span(),
                    fix: None,
                });
                continue;
            }
            let enums: Vec<som_ast::Id<som_ast::Struct>> = (ast.structs.iter())
                .filter(|(_, s)| s.is_enum())
                .map(|(id, _)| id)
                .collect();
            let mut copies = Vec::new();
            for (k, e) in enums.into_iter().enumerate() {
                let written = match write_copies(ast, &r, e) {
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
                let mut all: Vec<som_ast::Arm> = (arms.iter())
                    .map(|a| som_ast::Arm {
                        pattern: a.pattern,
                        body: a.body,
                    })
                    .collect();
                all.splice(at.min(all.len())..at.min(all.len()), new_arms);
                let span = ast.exprs.span(expr).clone();
                let copy = ast.exprs.alloc(
                    Expr::Match {
                        scrutinee,
                        arms: all,
                    },
                    span,
                );
                copies.push((e, copy));
            }
            ast.per_type_matches.insert(
                expr,
                som_ast::PerType {
                    of: r.of.clone(),
                    arms: copies,
                },
            );
            continue;
        }
        // Not a type there is: in a function, a generic one's, `T`. Written out for every
        // struct, the function's instances each picking theirs (whether `T` is one is the
        // type checker's to say).
        if found.is_none()
            && r.of.segments.len() == 1
            && let ReflectedInto::Stmts { stmt } = r.into
        {
            let structs: Vec<som_ast::Id<som_ast::Struct>> = (ast.structs.iter())
                .filter(|(_, s)| !s.is_enum())
                .map(|(id, _)| id)
                .collect();
            let mut arms = Vec::new();
            for (k, s) in structs.into_iter().enumerate() {
                let written = match write_copies(ast, &r, s) {
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
            ast.per_type.insert(
                stmt,
                som_ast::PerType {
                    of: r.of.clone(),
                    arms,
                },
            );
            continue;
        }
        let Some(s) = found else {
            errors.push(ParseError {
                message: msg![
                    "there's no enum or struct ",
                    code(name.to_string()),
                    " to go through"
                ],
                hint: msg![
                    code("@for v in Kind"),
                    " goes through an enum's variants, ",
                    code("@for f in Point"),
                    " a struct's fields; not yet a generic type's"
                ],
                span: r.of.span(),
                fix: None,
            });
            continue;
        };
        let written = match write_copies(ast, &r, s) {
            Ok(written) => written,
            Err(error) => {
                errors.push(error);
                continue;
            }
        };

        let (expr, at) = match r.into {
            ReflectedInto::Arms { expr, at } => (expr, at),
            ReflectedInto::Stmts { stmt } => {
                errors.extend(write_stmts(ast, r.module, written, stmt, r.body.clone()));
                continue;
            }
        };
        let (arms, arm_errors) = parse_arms(ast, r.module, written);
        errors.extend(arm_errors);
        if let Expr::Match { arms: existing, .. } = ast.exprs.get_mut(expr) {
            existing.splice(at..at, arms);
        }
    }
    errors
}

/// `tokens` parsed as a `match`'s arms.
fn parse_arms(
    ast: &mut Ast,
    module: usize,
    tokens: Vec<Token>,
) -> (Vec<som_ast::Arm>, Vec<ParseError>) {
    let mut parser = Parser::new(tokens, ast, module);
    parser.skip_trivia();
    let mut arms = Vec::new();
    while !parser.at_eof() {
        let Some(pattern) = parser.parse_pattern() else {
            break;
        };
        if parser
            .expect(TokenKind::FatArrow, "`=>` and the arm's value")
            .is_none()
        {
            break;
        }
        let Some(body) = parser.parse_expr() else {
            break;
        };
        arms.push(som_ast::Arm { pattern, body });
        let ends_in_block = matches!(parser.ast.exprs[body], Expr::Block { .. });
        if !parser.eat(TokenKind::Comma) && !ends_in_block {
            break;
        }
    }
    let errors = std::mem::take(&mut parser.errors);
    (arms, errors)
}

/// The copies' statements, `tokens`, parsed and put in place of statement `stmt`: a block of
/// them.
fn write_stmts(
    ast: &mut Ast,
    module: usize,
    tokens: Vec<Token>,
    stmt: som_ast::Id<som_ast::Stmt>,
    span: Span,
) -> Vec<ParseError> {
    let (block, errors) = parse_block(ast, module, tokens, span);
    *ast.stmts.get_mut(stmt) = som_ast::Stmt::Expr(block);
    errors
}

/// `tokens` parsed as statements: a block of them.
fn parse_block(
    ast: &mut Ast,
    module: usize,
    tokens: Vec<Token>,
    span: Span,
) -> (som_ast::Id<Expr>, Vec<ParseError>) {
    let mut parser = Parser::new(tokens, ast, module);
    parser.skip_trivia();
    let mut stmts = Vec::new();
    while !parser.at_eof() {
        let before = parser.pos;
        match parser.parse_stmt_or_expr() {
            crate::StmtOrExpr::Stmt(s) => stmts.push(s),
            crate::StmtOrExpr::Expr(e) => {
                let span = parser.ast.exprs.span(e).clone();
                stmts.push(parser.ast.stmts.alloc(som_ast::Stmt::Expr(e), span));
                parser.eat(TokenKind::Semicolon);
            }
            crate::StmtOrExpr::Error => parser.recover(crate::RecoveryLevel::Statement),
        }
        if parser.pos == before {
            parser.advance();
        }
    }
    let errors = std::mem::take(&mut parser.errors);
    let block = ast.exprs.alloc(Expr::Block { stmts, value: None }, span);
    (block, errors)
}

/// The tokens of `r`'s copies for enum or struct `s`: its body once for each variant or
/// field, the element written as it (and its index, if it's named), then an `Eof`.
fn write_copies(
    ast: &Ast,
    r: &som_ast::Reflected,
    s: som_ast::Id<som_ast::Struct>,
) -> Result<Vec<Token>, ParseError> {
    // What each copy writes for the element: a variant's pattern, or a field's name.
    let strukt = &ast.structs[s];
    let elements: Vec<String> = match strukt.is_enum() {
        true => strukt.variants().iter().map(variant_pattern).collect(),
        false => (strukt.fields.iter())
            .map(|f| f.name.name.to_string())
            .collect(),
    };

    // The body's tokens, as written.
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
            return Err(ParseError {
                message: msg!["an ", code("@for"), " over a type names one or two things"],
                hint: msg![code("v"), ", or its index and it: ", code("(i, v)")],
                span: r.names[0].span.clone(),
                fix: None,
            });
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
                // The variant's pattern, or the field's name, said to be where `v` is.
                let pattern = lex(std::sync::Arc::new(Source::from_raw(pattern.as_str())));
                written.extend(
                    pattern
                        .into_iter()
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
