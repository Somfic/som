use som_ast::{Arm, Expr, Id, Ident, Path, Pattern, PatternFields, Reflected, ReflectedInto};
use som_base::message::code;
use som_base::msg;
use som_lang::Builtin;
use som_lexer::TokenKind;

use crate::Parser;

const WILDCARD: &str = "_";

impl Parser<'_> {
    /// `match scrutinee { pattern => value, … }`.
    pub(crate) fn parse_match(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        self.expect(TokenKind::Match, "a match")?;
        let scrutinee = self.parse_condition()?;
        self.expect(TokenKind::OpenBrace, "the match arms")?;
        let mut arms = Vec::new();
        // `@for (i, v) in Kind { v => i, }`: written out once `Kind` is known, before the arm
        // at the index each has.
        let mut reflected = Vec::new();
        self.nested(|p| {
            while !p.at(TokenKind::CloseBrace) && !p.at_eof() {
                if p.at_reflected_for() {
                    let start = p.current_span();
                    match p.parse_reflected_for() {
                        Some(found) => reflected.push((arms.len(), found)),
                        // Reported: an arm matching anything stands in, so nothing else is
                        // said of a `match` it leaves without arms.
                        None => {
                            let span = p.span_from(&start);
                            let pattern = p.ast.patterns.alloc(Pattern::Wildcard, span.clone());
                            let body = p.ast.exprs.alloc(Expr::Hole, span);
                            arms.push(Arm { pattern, body });
                        }
                    }
                    p.eat(TokenKind::Comma);
                    continue;
                }
                if p.at_builtin(Builtin::For) {
                    p.expand_for();
                    continue;
                }
                let arm = p.parse_arm()?;
                let more = p.arm_continues(&arm);
                arms.push(arm);
                if !more {
                    break;
                }
            }
            Some(())
        })?;
        self.expect(
            TokenKind::CloseBrace,
            "a closing brace after the match arms",
        )?;
        let id = self.alloc_expr(Expr::Match { scrutinee, arms }, &start);
        for (at, (names, of, leaves, body)) in reflected {
            self.ast.reflected.push(Reflected {
                module: self.module,
                names,
                of,
                leaves,
                body,
                into: ReflectedInto::Arms { expr: id, at },
            });
        }
        Some(id)
    }

    /// `pattern => value`.
    pub(crate) fn parse_arm(&mut self) -> Option<Arm> {
        let pattern = self.parse_pattern()?;
        self.expect(TokenKind::FatArrow, "`=>` and the arm's value")?;
        let body = self.parse_expr()?;
        Some(Arm { pattern, body })
    }

    /// Past the `,` after `arm`, if there is one: whether another arm can follow. An arm whose
    /// value is a block needs no comma.
    pub(crate) fn arm_continues(&mut self, arm: &Arm) -> bool {
        let ends_in_block = matches!(self.ast.exprs[arm.body], Expr::Block { .. });
        self.eat(TokenKind::Comma) || ends_in_block
    }

    /// `_`, `name`, `3`, `true`, `"yes"`, `none`, `Circle(r)`, `Rect { w, h }` or
    /// `Shape::Empty`.
    pub(crate) fn parse_pattern(&mut self) -> Option<Id<Pattern>> {
        let start = self.current_span();
        let pattern = match self.peek() {
            TokenKind::Int | TokenKind::Minus => self.parse_int_pattern()?,
            TokenKind::True | TokenKind::False => {
                let value = self.at(TokenKind::True);
                self.advance();
                Pattern::Bool(value)
            }
            TokenKind::Text => self.parse_text_pattern(),
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
                self.parse_path_pattern(path)?
            }
            _ => {
                self.error_missing("expected a pattern", "a pattern");
                return None;
            }
        };
        let span = self.span_from(&start);
        Some(self.ast.patterns.alloc(pattern, span))
    }

    fn parse_int_pattern(&mut self) -> Option<Pattern> {
        let negative = self.eat(TokenKind::Minus);
        let span = self.current_span();
        let count = self.parse_count("a number")?;
        match i128::try_from(count) {
            Ok(n) => Some(Pattern::Int(if negative { -n } else { n })),
            Err(_) => {
                self.error_at(
                    span,
                    "integer literal is too large",
                    "this does not fit in 128 bits",
                );
                Some(Pattern::Wildcard)
            }
        }
    }

    /// `"yes"`: plain text. A value in it, `"{x}"`, isn't known until the program runs.
    fn parse_text_pattern(&mut self) -> Pattern {
        let token = self.peek_token().clone();
        self.advance();
        match self.text_literal(&token) {
            Expr::Str(text) => Pattern::Str(text),
            _ => {
                self.error_at(
                    token.span.clone(),
                    "a pattern's text can't hold values",
                    msg!["for a brace itself, write ", code("{{"), " or ", code("}}")],
                );
                Pattern::Wildcard
            }
        }
    }

    /// After a pattern's `path`: a variant and what it holds, `_`, or a name to bind.
    fn parse_path_pattern(&mut self, path: Path) -> Option<Pattern> {
        if self.eat(TokenKind::OpenParen) {
            let fields = self.parse_comma_list(TokenKind::CloseParen, |p| p.parse_pattern())?;
            self.expect(TokenKind::CloseParen, "a closing parenthesis")?;
            return Some(Pattern::Variant {
                path,
                fields: PatternFields::Tuple(fields),
            });
        }
        if self.eat(TokenKind::OpenBrace) {
            let fields = self.parse_named_field_patterns()?;
            return Some(Pattern::Variant { path, fields });
        }
        if path.segments.len() > 1 {
            return Some(Pattern::Variant {
                path,
                fields: PatternFields::Unit,
            });
        }
        if &*path.segments[0].name == WILDCARD {
            return Some(Pattern::Wildcard);
        }
        let name = path
            .segments
            .into_iter()
            .next()
            .expect("paths aren't empty");
        Some(Pattern::Binding {
            name,
            mutable: false,
        })
    }

    /// `w, h: height, .. }` after the `{` of a variant pattern. `w` is short for `w: w`.
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
                let binding = Pattern::Binding {
                    name: name.clone(),
                    mutable: false,
                };
                self.ast.patterns.alloc(binding, name.span.clone())
            };
            fields.push((name, pattern));
            if !self.list_continues(TokenKind::CloseBrace) {
                break;
            }
        }
        self.expect(TokenKind::CloseBrace, "a closing brace after the fields")?;
        Some(PatternFields::Named { fields, rest })
    }
}
