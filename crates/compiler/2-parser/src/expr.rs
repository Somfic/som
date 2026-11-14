use som_ast::{Arm, Expr, Id, Ident, Pattern, PatternFields, Span};
use som_base::message::code;
use som_base::msg;
use som_lexer::TokenKind;

use crate::grammar::Grammar;
use crate::Parser;

impl Parser<'_> {
    pub(crate) fn parse_expr(&mut self) -> Option<Id<Expr>> {
        self.parse_expr_bp(0)
    }

    /// The condition of an `if` or `while`, or the optional of an `if let`: `x { … }` there
    /// is `x` followed by the body. Write `(Point { … })` for a constructor.
    pub(crate) fn parse_condition(&mut self) -> Option<Id<Expr>> {
        self.with_condition(true, |p| p.parse_expr())
    }

    fn with_condition<T>(&mut self, in_condition: bool, f: impl FnOnce(&mut Self) -> T) -> T {
        let outer = std::mem::replace(&mut self.in_condition, in_condition);
        let result = f(self);
        self.in_condition = outer;
        result
    }

    fn alloc_expr(&mut self, expr: Expr, start: &Span) -> Id<Expr> {
        let span = self.span_from(start);
        self.ast.exprs.alloc(expr, span)
    }

    /// Pratt parser: parses an expression whose operators all bind at least `min_bp` tightly.
    fn parse_expr_bp(&mut self, min_bp: u8) -> Option<Id<Expr>> {
        let start = self.current_span();
        let mut lhs = self.parse_prefix_or_atom()?;

        loop {
            // `.field` or `.method(args)`
            if self.at(TokenKind::Dot) {
                if Grammar::POSTFIX * 2 < min_bp {
                    break;
                }
                self.advance();
                // `alloc(10).` while typing: report the missing name, but keep what's
                // before the dot, so an editor still knows its type and can list fields.
                let Some(name) = self.parse_ident("a field or method name") else {
                    break;
                };
                lhs = if self.at(TokenKind::OpenParen) {
                    let args = self.parse_args()?;
                    self.alloc_expr(
                        Expr::MethodCall {
                            receiver: lhs,
                            method: name,
                            args,
                        },
                        &start,
                    )
                } else {
                    self.alloc_expr(
                        Expr::Field {
                            object: lhs,
                            field: name,
                        },
                        &start,
                    )
                };
                continue;
            }

            // `values[0]`
            if self.at(TokenKind::OpenBracket) {
                if Grammar::POSTFIX * 2 < min_bp {
                    break;
                }
                self.advance();
                let index = self.parse_expr()?;
                self.expect(TokenKind::CloseBracket, "a closing bracket after the index")?;
                lhs = self.alloc_expr(Expr::Index { object: lhs, index }, &start);
                continue;
            }

            // `callee(args)`
            if self.at(TokenKind::OpenParen) {
                if Grammar::POSTFIX * 2 < min_bp {
                    break;
                }
                lhs = self.parse_call(lhs, &start)?;
                continue;
            }

            // `then if condition else otherwise`
            if self.at(TokenKind::If) {
                if 1 < min_bp {
                    break;
                }
                self.advance();
                // Stops before `else`, which belongs to this conditional, not to a fallback.
                let condition = self.parse_expr_bp(Grammar::FALLBACK + 1)?;
                self.expect(TokenKind::Else, "an else branch")?;
                let otherwise = self.parse_expr()?;
                lhs = self.alloc_expr(
                    Expr::Conditional {
                        condition,
                        then: lhs,
                        otherwise,
                    },
                    &start,
                );
                continue;
            }

            // `optional else fallback`
            if self.at(TokenKind::Else) {
                if Grammar::FALLBACK < min_bp {
                    break;
                }
                self.advance();
                let fallback = self.parse_expr_bp(Grammar::FALLBACK)?;
                lhs = self.alloc_expr(
                    Expr::Fallback {
                        optional: lhs,
                        fallback,
                    },
                    &start,
                );
                continue;
            }

            // `target = value`
            if self.at(TokenKind::Equals) {
                if Grammar::ASSIGNMENT < min_bp {
                    break;
                }
                self.advance();
                let value = self.parse_expr_bp(Grammar::ASSIGNMENT)?;
                lhs = self.alloc_expr(Expr::Assign { target: lhs, value }, &start);
                continue;
            }

            // `value as ty`
            if self.at(TokenKind::As) {
                if Grammar::AS * 2 < min_bp {
                    break;
                }
                self.advance();
                let ty = self.parse_type()?;
                lhs = self.alloc_expr(Expr::Cast { value: lhs, ty }, &start);
                continue;
            }

            // `start..end`, or `start..` with the end left open.
            if self.at(TokenKind::DotDot) {
                if Grammar::RANGE * 2 < min_bp {
                    break;
                }
                self.advance();
                let end = if self.can_start_expr() {
                    Some(self.parse_expr_bp(Grammar::RANGE * 2 + 1)?)
                } else {
                    None
                };
                lhs = self.alloc_expr(
                    Expr::Range {
                        start: Some(lhs),
                        end,
                    },
                    &start,
                );
                continue;
            }

            let Some((op, info)) = Grammar::infix_op(self.peek()) else {
                break;
            };
            let (lbp, rbp) = info.binding_power();
            if lbp < min_bp {
                break;
            }
            self.advance();
            let rhs = self.parse_expr_bp(rbp)?;
            lhs = self.alloc_expr(Expr::Binary { op, lhs, rhs }, &start);
        }

        Some(lhs)
    }

    fn parse_prefix_or_atom(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        let kind = self.peek();

        // `..end`, or `..`: a range from the start.
        if kind == TokenKind::DotDot {
            self.advance();
            let end = if self.can_start_expr() {
                Some(self.parse_expr_bp(Grammar::RANGE * 2 + 1)?)
            } else {
                None
            };
            return Some(self.alloc_expr(Expr::Range { start: None, end }, &start));
        }

        let Some(bp) = Grammar::prefix_bp(kind) else {
            return self.parse_atom();
        };

        self.advance();
        let mutable = kind == TokenKind::Ampersand && self.eat(TokenKind::Mut);
        let expr = self.parse_expr_bp(bp)?;
        let expr = match kind {
            TokenKind::Bang => Expr::Not(expr),
            TokenKind::Ampersand => Expr::Borrow { mutable, expr },
            TokenKind::Star => Expr::Deref(expr),
            // `-1` is the literal `-1`, not `1` negated: so it can be the smallest `i64`, and
            // a `u8` can't be it.
            TokenKind::Minus => match self.ast.exprs[expr] {
                Expr::Int(n) if self.ast.exprs.span(expr).start_offset == start.start_offset + 1 => {
                    Expr::Int(n.wrapping_neg())
                }
                Expr::Float(x) if self.ast.exprs.span(expr).start_offset == start.start_offset + 1 => {
                    Expr::Float(-x)
                }
                _ => Expr::Neg(expr),
            },
            _ => unreachable!("prefix_bp only accepts prefix operators"),
        };
        Some(self.alloc_expr(expr, &start))
    }

    fn parse_atom(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();

        let expr = match self.peek() {
            TokenKind::Int => {
                let text = self.peek_token().text.clone();
                self.advance();
                match text.parse() {
                    Ok(value) => Expr::Int(value),
                    Err(_) => {
                        self.error_at(
                            start.clone(),
                            "integer literal is too large",
                            "this does not fit in 64 bits",
                        );
                        Expr::Hole
                    }
                }
            }

            TokenKind::Float => {
                let value = self.peek_token().text.parse().unwrap_or(0.0);
                self.advance();
                Expr::Float(value)
            }

            TokenKind::None => {
                self.advance();
                Expr::None
            }

            // `[a, b, c]` or `[value; count]`
            TokenKind::OpenBracket => {
                self.advance();
                if self.eat(TokenKind::CloseBracket) {
                    Expr::Array(Vec::new())
                } else {
                    let first = self.with_condition(false, |p| p.parse_expr())?;
                    if self.eat(TokenKind::Semicolon) {
                        let count = self.parse_length("how many copies")?;
                        self.expect(TokenKind::CloseBracket, "a closing bracket")?;
                        Expr::ArrayRepeat {
                            value: first,
                            count,
                        }
                    } else {
                        let mut items = vec![first];
                        while self.eat(TokenKind::Comma) && !self.at(TokenKind::CloseBracket) {
                            items.push(self.with_condition(false, |p| p.parse_expr())?);
                        }
                        self.expect(
                            TokenKind::CloseBracket,
                            "a closing bracket after the elements",
                        )?;
                        Expr::Array(items)
                    }
                }
            }

            TokenKind::True | TokenKind::False => {
                let value = self.at(TokenKind::True);
                self.advance();
                Expr::Bool(value)
            }

            TokenKind::Text => {
                let text = self.peek_token().text.clone();
                self.advance();
                Expr::Str(self.unescape(&text[1..text.len() - 1], &start).into())
            }

            // `i32::parse(…)`: a built-in type's own function.
            kind if crate::ty::primitive(kind).is_some()
                && self.peek_nth(1) == TokenKind::DoubleColon =>
            {
                let token = self.peek_token().clone();
                self.advance();
                let mut segments = vec![som_ast::Ident {
                    name: token.text.as_ref().into(),
                    span: token.span.clone(),
                }];
                while self.eat(TokenKind::DoubleColon) {
                    segments.push(self.parse_ident("a function name")?);
                }
                Expr::Var(som_ast::Path { segments })
            }
            TokenKind::Ident => {
                // `Name { field: ...` or `Name {}` is a constructor, and so is
                // `module::Name { ... }`; `name { ... }` is not.
                let mut n = 1;
                while self.peek_nth(n) == TokenKind::DoubleColon
                    && self.peek_nth(n + 1) == TokenKind::Ident
                {
                    n += 2;
                }
                // `Pair<i32> { … }`: type arguments before the brace.
                if self.peek_nth(n) == TokenKind::LessThan {
                    n = self.past_type_args(n).unwrap_or(n);
                }
                let is_constructor = self.peek_nth(n) == TokenKind::OpenBrace
                    && (self.peek_nth(n + 1) == TokenKind::CloseBrace
                        || (self.peek_nth(n + 1) == TokenKind::Ident
                            && self.peek_nth(n + 2) == TokenKind::Colon));
                if is_constructor && !self.in_condition {
                    return self.parse_constructor();
                }
                Expr::Var(self.parse_path("an identifier or constructor name")?)
            }

            TokenKind::OpenParen => {
                self.advance();
                let inner = self.with_condition(false, |p| p.parse_expr())?;
                self.expect(TokenKind::CloseParen, "closing parenthesis")?;
                return Some(inner);
            }

            TokenKind::OpenBrace => return self.with_condition(false, |p| p.parse_block()),

            TokenKind::Match => return self.parse_match(),
            // `if c { a } else { b }`, giving a value.
            TokenKind::If => {
                let parts = self.parse_if_parts()?;
                return Some(self.if_expr(parts));
            }

            _ => {
                self.error_missing("expected expression", "an expression");
                return Some(self.ast.exprs.alloc(Expr::Hole, start));
            }
        };

        Some(self.alloc_expr(expr, &start))
    }

    /// Whether an expression can start here: after `..`, whether a range has an end.
    fn can_start_expr(&self) -> bool {
        !matches!(
            self.peek(),
            TokenKind::CloseParen
                | TokenKind::CloseBracket
                | TokenKind::CloseBrace
                | TokenKind::OpenBrace
                | TokenKind::Comma
                | TokenKind::Semicolon
                | TokenKind::Else
                | TokenKind::Eof
        )
    }

    /// A count written as an integer literal: an array's length, or how many copies.
    pub(crate) fn parse_count(&mut self, what: &str) -> Option<u128> {
        let span = self.current_span();
        if !self.at(TokenKind::Int) {
            self.error_expected(&[TokenKind::Int], what);
            return None;
        }
        let text = self.peek_token().text.clone();
        self.advance();
        match text.parse() {
            Ok(count) => Some(count),
            Err(_) => {
                self.error_at(span, "count is too large", "this does not fit in 128 bits");
                None
            }
        }
    }

    /// The text of a string literal with its escapes resolved: `\n`, `\t`, `\r`, `\0`,
    /// `\\`, `\"` and `\'`. An unknown escape is reported, and kept as written.
    fn unescape(&mut self, raw: &str, span: &Span) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('0') => out.push('\0'),
                Some(c @ ('\\' | '"' | '\'')) => out.push(c),
                other => {
                    let escape = format!("\\{}", other.map(String::from).unwrap_or_default());
                    self.error_at(
                        span.clone(),
                        msg!["unknown escape ", code(&escape)],
                        msg!["expected one of ", code("\\n \\t \\r \\0 \\\\ \\\" \\'")],
                    );
                    out.push_str(&escape);
                }
            }
        }
        out
    }

    /// An argument: an expression, or `out name`.
    fn parse_arg(&mut self) -> Option<Id<Expr>> {
        if !self.at(TokenKind::Out) {
            return self.parse_expr();
        }
        let start = self.current_span();
        self.advance();
        let name = self.parse_ident("a name for the out variable")?;
        Some(self.alloc_expr(Expr::Out(name), &start))
    }

    /// `(a, b, c)`
    fn parse_args(&mut self) -> Option<Vec<Id<Expr>>> {
        self.expect(TokenKind::OpenParen, "a list of arguments")?;
        let args = self.with_condition(false, |p| {
            p.parse_comma_list(TokenKind::CloseParen, |p| p.parse_arg())
        })?;
        // `free(alloc(10).` while typing: report the missing `)`, but keep the call and
        // its arguments, so an editor still knows them.
        if !self.eat(TokenKind::CloseParen) {
            self.error_expected(
                &[TokenKind::CloseParen],
                "a closing parenthesis after the arguments",
            );
        }
        Some(args)
    }

    /// Only named functions can be called: `foo(..)` or `math::add(..)`.
    fn parse_call(&mut self, callee: Id<Expr>, start: &Span) -> Option<Id<Expr>> {
        let args = self.parse_args()?;
        let expr = match &self.ast.exprs[callee] {
            Expr::Var(path) => Expr::Call {
                callee: path.clone(),
                args,
            },
            _ => {
                self.error_missing("expected function name", "a function name");
                Expr::Hole
            }
        };
        // The callee was parsed as a variable; turn that node into the call, so no
        // unreferenced node is left behind.
        let span = self.span_from(start);
        self.ast.exprs.replace(callee, expr, span);
        Some(callee)
    }

    /// `{ stmts; value }`
    pub(crate) fn parse_block(&mut self) -> Option<Id<Expr>> {
        let block = self.parse_block_parts("the block")?;
        Some(self.block_expr(block))
    }

    /// How far ahead the `>` closing type arguments that open at `n` (a `<`) is, plus one; `None`
    /// if what's there can't be type arguments, as in `a < b`. Only looks ahead.
    fn past_type_args(&self, mut n: usize) -> Option<usize> {
        let mut depth = 0;
        loop {
            match self.peek_nth(n) {
                TokenKind::LessThan => depth += 1,
                TokenKind::GreaterThan => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(n + 1);
                    }
                }
                // What a type can be made of.
                TokenKind::Ident
                | TokenKind::DoubleColon
                | TokenKind::Comma
                | TokenKind::Ampersand
                | TokenKind::Mut
                | TokenKind::Question
                | TokenKind::OpenBracket
                | TokenKind::CloseBracket
                | TokenKind::Semicolon
                | TokenKind::Int
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Plus
                | TokenKind::Star
                | TokenKind::I8
                | TokenKind::I16
                | TokenKind::I32
                | TokenKind::I64
                | TokenKind::I128
                | TokenKind::ISize
                | TokenKind::U8
                | TokenKind::U16
                | TokenKind::U32
                | TokenKind::U64
                | TokenKind::U128
                | TokenKind::USize
                | TokenKind::F32
                | TokenKind::F64
                | TokenKind::Bool
                | TokenKind::Char
                | TokenKind::Str
                | TokenKind::CStr => {}
                _ => return None,
            }
            n += 1;
        }
    }

    /// `match scrutinee { pattern => value, … }`. An arm whose value is a block needs no comma
    /// after it.
    pub(crate) fn parse_match(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        self.expect(TokenKind::Match, "a match")?;
        let scrutinee = self.parse_condition()?;
        self.expect(TokenKind::OpenBrace, "the match arms")?;
        let mut arms = Vec::new();
        self.with_condition(false, |p| {
            while !p.at(TokenKind::CloseBrace) && !p.at_eof() {
                let pattern = p.parse_pattern()?;
                p.expect(TokenKind::FatArrow, "`=>` and the arm's value")?;
                let body = p.parse_expr()?;
                arms.push(Arm { pattern, body });
                let ends_in_block = matches!(p.ast.exprs[body], Expr::Block { .. });
                if !p.eat(TokenKind::Comma) && !ends_in_block {
                    break;
                }
            }
            Some(())
        })?;
        self.expect(
            TokenKind::CloseBrace,
            "a closing brace after the match arms",
        )?;
        let scrutinee_and_arms = Expr::Match { scrutinee, arms };
        Some(self.alloc_expr(scrutinee_and_arms, &start))
    }

    /// A `match` arm's pattern: `_`, `name`, `3`, `true`, `none`, `Circle(r)`,
    /// `Rect { w, h }` or `Shape::Empty`.
    fn parse_pattern(&mut self) -> Option<Id<Pattern>> {
        let start = self.current_span();
        let pattern = match self.peek() {
            TokenKind::Int | TokenKind::Minus => {
                let negative = self.eat(TokenKind::Minus);
                let span = self.current_span();
                let count = self.parse_count("a number")?;
                match i64::try_from(count) {
                    Ok(n) => Pattern::Int(if negative { -n } else { n }),
                    Err(_) => {
                        self.error_at(
                            span,
                            "integer literal is too large",
                            "this does not fit in 64 bits",
                        );
                        Pattern::Wildcard
                    }
                }
            }
            TokenKind::True | TokenKind::False => {
                let value = self.at(TokenKind::True);
                self.advance();
                Pattern::Bool(value)
            }
            TokenKind::None => {
                self.advance();
                Pattern::None
            }
            TokenKind::Mut => {
                self.advance();
                let name = self.parse_ident("a name to bind")?;
                Pattern::Binding {
                    name,
                    mutable: true,
                }
            }
            TokenKind::Ident => {
                let path = self.parse_path("a pattern")?;
                if self.eat(TokenKind::OpenParen) {
                    let fields =
                        self.parse_comma_list(TokenKind::CloseParen, |p| p.parse_pattern())?;
                    self.expect(TokenKind::CloseParen, "a closing parenthesis")?;
                    Pattern::Variant {
                        path,
                        fields: PatternFields::Tuple(fields),
                    }
                } else if self.eat(TokenKind::OpenBrace) {
                    let fields = self.parse_named_field_patterns()?;
                    Pattern::Variant { path, fields }
                } else if path.segments.len() > 1 {
                    Pattern::Variant {
                        path,
                        fields: PatternFields::Unit,
                    }
                } else if &*path.segments[0].name == "_" {
                    Pattern::Wildcard
                } else {
                    let name = path
                        .segments
                        .into_iter()
                        .next()
                        .expect("paths aren't empty");
                    Pattern::Binding {
                        name,
                        mutable: false,
                    }
                }
            }
            _ => {
                self.error_missing("expected a pattern", "a pattern");
                return None;
            }
        };
        let span = self.span_from(&start);
        Some(self.ast.patterns.alloc(pattern, span))
    }

    /// `w, h: height, .. }` after the `{` of a variant pattern.
    fn parse_named_field_patterns(&mut self) -> Option<PatternFields> {
        let mut fields = Vec::new();
        let mut rest = false;
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.eat(TokenKind::DotDot) {
                rest = true;
                break;
            }
            let name: Ident = self.parse_ident("a field name")?;
            let pattern = if self.eat(TokenKind::Colon) {
                self.parse_pattern()?
            } else {
                // `w` is short for `w: w`.
                let binding = Pattern::Binding {
                    name: name.clone(),
                    mutable: false,
                };
                self.ast.patterns.alloc(binding, name.span.clone())
            };
            fields.push((name, pattern));
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::CloseBrace, "a closing brace after the fields")?;
        Some(PatternFields::Named { fields, rest })
    }

    /// `Point { x: 1, y: 2 }`, or `Pair<i32> { a: 1, b: 2 }`
    fn parse_constructor(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        let ty = self.parse_path("a constructor name")?;
        let args = self.parse_type_args()?;

        self.expect(TokenKind::OpenBrace, "constructor fields")?;
        let fields = self.parse_comma_list(TokenKind::CloseBrace, |p| {
            let name = p.parse_ident("a constructor field name")?;
            p.expect(TokenKind::Colon, "a constructor field value")?;
            let value = p.parse_expr()?;
            Some((name, value))
        })?;
        self.expect(
            TokenKind::CloseBrace,
            "a closing brace after constructor fields",
        )?;

        Some(self.alloc_expr(Expr::Construct { ty, args, fields }, &start))
    }
}
