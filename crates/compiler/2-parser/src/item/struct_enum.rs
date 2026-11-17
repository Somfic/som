use som_ast::{Field, Id, Ident, Struct, Variant, VariantKind, Visibility};
use som_base::message::code;
use som_base::{Fix, Span, msg};
use som_lexer::TokenKind;

use crate::Parser;
use crate::grammar::VISIBILITY;
use crate::ty::primitive;

/// A method written among a struct's fields: all of it, where it starts with the space
/// before it, and its header, up to its name.
struct MemberMethod {
    span: Span,
    from: Span,
    header: Span,
}

impl Parser<'_> {
    /// `struct Point { x: i32, y: i32 }`, or `extern struct Sqlite3;`: a struct only C
    /// sees inside, used through handles to one.
    pub(super) fn parse_struct(&mut self) -> Option<Id<Struct>> {
        let doc = self.doc_comment();
        let visibility = std::mem::take(&mut self.visibility);
        let start = self.current_span();
        let handle = self.eat(TokenKind::Extern);
        self.expect_struct("a struct declaration")?;
        let name = self.parse_ident("a struct name")?;
        let generics = self.parse_generics(true)?;
        let fields = match handle {
            true => {
                self.expect(TokenKind::Semicolon, "a semicolon after an extern struct")?;
                Vec::new()
            }
            false => self.parse_struct_fields(&name.name)?,
        };
        let span = self.span_from(&start);
        let s = Struct {
            doc,
            visibility,
            name,
            generics,
            fields,
            variants: None,
            // A handle is just a pointer, so copying it copies nothing C owns.
            copy: handle,
            handle,
        };
        Some(self.ast.structs.alloc(s, span))
    }

    /// `{ x: i32, y: i32 }`. Methods written among the fields, as in another language's
    /// class, are skipped and reported, to move to an `impl` block.
    fn parse_struct_fields(&mut self, name: &str) -> Option<Vec<Field>> {
        self.expect(TokenKind::OpenBrace, "a list of struct fields")?;
        let mut methods = Vec::new();
        let mut constructor = false;
        let mut fields = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if let Some((is_constructor, name_at)) = self.member_method(name) {
                let start = self.current_span();
                let header = start.merge(&self.span_ahead(name_at));
                let from = self.previous_span().after();
                self.skip_member();
                constructor |= is_constructor;
                methods.push(MemberMethod {
                    span: self.span_from(&start),
                    from,
                    header,
                });
                continue;
            }
            let before = self.pos;
            let (field, separated) = self.parse_field()?;
            fields.push(field);
            if self.pos == before {
                return None;
            }
            if !separated && !self.list_continues(TokenKind::CloseBrace) {
                break;
            }
        }
        let close = self.current_span();
        self.expect(TokenKind::CloseBrace, "a closing brace after struct fields")?;
        if !methods.is_empty() {
            self.methods_in_struct(name, &methods, constructor, &close);
        }
        Some(fields)
    }

    /// A struct field, `name: Type`, or another language's `Type name;`, reported and read as
    /// the `name: Type,` it means. Whether that took its separator, the `;`, along.
    fn parse_field(&mut self) -> Option<(Field, bool)> {
        let doc = self.doc_comment();
        let visibility = self.parse_visibility();
        let type_first = (self.at(TokenKind::Ident) || primitive(self.peek()).is_some())
            && self.peek_nth(1) == TokenKind::Ident
            && matches!(
                self.peek_nth(2),
                TokenKind::Semicolon | TokenKind::Comma | TokenKind::CloseBrace
            );
        if !type_first {
            let name = self.parse_ident("a struct field name")?;
            self.expect(TokenKind::Colon, "a struct field type")?;
            let ty = self.parse_type()?;
            let field = Field {
                doc,
                visibility,
                name,
                ty,
            };
            return Some((field, false));
        }
        let start = self.current_span();
        let ty = self.parse_type()?;
        let ty_text = self.span_from(&start).get_text();
        let name = self.parse_ident("a struct field name")?;
        let separated = self.eat(TokenKind::Semicolon) || self.eat(TokenKind::Comma);
        let span = self.span_from(&start);
        let fixed = format!(
            "{}: {ty_text}{}",
            name.name,
            if separated { "," } else { "" }
        );
        self.report(
            span.clone(),
            msg!["som writes a field's name first, then its type"],
            msg!["write it ", code(&fixed)],
            Some(Fix::change_to(span, fixed)),
        );
        let field = Field {
            doc,
            visibility,
            name,
            ty,
        };
        Some((field, separated))
    }

    /// At a method among a struct's fields, as in another language's class: `fn area(…)`,
    /// `int area(…)`, or a constructor, `Point(…)`, after any modifiers. Whether it's a
    /// constructor, and how many tokens ahead its name is.
    fn member_method(&self, struct_name: &str) -> Option<(bool, usize)> {
        let mut at = 0;
        while VISIBILITY.contains(&self.peek_nth(at)) || self.foreign_modifier(at).is_some() {
            at += 1;
        }
        if self.ident_ahead(at) == Some(struct_name)
            && self.peek_nth(at + 1) == TokenKind::OpenParen
        {
            return Some((true, at));
        }
        (self.peek_nth(at) == TokenKind::Fn || self.foreign_fn(at) || self.c_style_fn(at))
            .then_some((false, at + 1))
    }

    /// Skips a member up to the end of its body, `{ … }`, or its `;`.
    fn skip_member(&mut self) {
        let mut depth = 0usize;
        while !self.at_eof() {
            match self.peek() {
                TokenKind::OpenParen | TokenKind::OpenBracket => depth += 1,
                TokenKind::CloseParen | TokenKind::CloseBracket => depth = depth.saturating_sub(1),
                TokenKind::Semicolon if depth == 0 => {
                    self.advance();
                    return;
                }
                TokenKind::OpenBrace if depth == 0 => {
                    self.skip_braces();
                    return;
                }
                // The struct's own `}`: a body never came.
                TokenKind::CloseBrace if depth == 0 => return,
                _ => {}
            }
            self.advance();
        }
    }

    /// Reports `methods` written among struct `name`'s fields. The fix moves them, as they
    /// are, to an `impl` block after the struct's closing brace, `close`.
    fn methods_in_struct(
        &mut self,
        name: &str,
        methods: &[MemberMethod],
        constructor: bool,
        close: &Span,
    ) {
        let mut edits = Vec::new();
        let mut moved = Vec::new();
        for method in methods {
            let removed = Span {
                end: method.span.end,
                length: method.span.start_offset + method.span.length - method.from.start_offset,
                ..method.from.clone()
            };
            edits.push((removed, String::new()));
            moved.push(format!("    {}", method.span.get_text()));
        }
        let block = format!("\n\nimpl {name} {{\n{}\n}}", moved.join("\n\n"));
        edits.push((close.after(), block));
        let hint = if constructor {
            msg![
                "a constructor is a function returning the struct: ",
                code(format!("fn new(…) -> {name} {{ {name} {{ … }} }}"))
            ]
        } else {
            msg![
                "a method takes ",
                code("self"),
                ", ",
                code("&self"),
                " or ",
                code("&mut self")
            ]
        };
        let fix = Fix {
            title: msg!["move them to ", code(format!("impl {name}"))],
            edits,
        };
        self.report(
            methods[0].header.clone(),
            msg![
                "a struct holds only its fields: methods go in ",
                code(format!("impl {name} {{ … }}"))
            ],
            hint,
            Some(fix),
        );
    }

    /// `enum Shape { Circle(f32), Rect { w: f32, h: f32 }, Empty }`.
    pub(super) fn parse_enum(&mut self) -> Option<Id<Struct>> {
        let doc = self.doc_comment();
        let visibility = std::mem::take(&mut self.visibility);
        let start = self.current_span();
        self.expect(TokenKind::Enum, "an enum declaration")?;
        let name = self.parse_ident("an enum name")?;
        let generics = self.parse_generics(true)?;
        self.expect(TokenKind::OpenBrace, "a list of enum variants")?;
        let variants = self.parse_comma_list(TokenKind::CloseBrace, |p| p.parse_variant())?;
        self.expect(TokenKind::CloseBrace, "a closing brace after the variants")?;
        // Variants that hold nothing are just a number.
        let copy = variants.iter().all(|v| v.fields.is_empty());
        let span = self.span_from(&start);
        let s = Struct {
            doc,
            visibility,
            name,
            generics,
            fields: Vec::new(),
            variants: Some(variants),
            copy,
            handle: false,
        };
        Some(self.ast.structs.alloc(s, span))
    }

    /// `Empty`, `Circle(f32)` or `Rect { w: f32, h: f32 }`, and the number it converts to.
    fn parse_variant(&mut self) -> Option<Variant> {
        let doc = self.doc_comment();
        let name = self.parse_ident("a variant name")?;
        let (kind, fields) = if self.eat(TokenKind::OpenParen) {
            (VariantKind::Tuple, self.parse_tuple_fields()?)
        } else if self.eat(TokenKind::OpenBrace) {
            (VariantKind::Named, self.parse_variant_fields()?)
        } else {
            (VariantKind::Unit, Vec::new())
        };
        let value = match self.at(TokenKind::Equals) {
            true => self.parse_variant_value(kind)?,
            false => None,
        };
        Some(Variant {
            doc,
            name,
            kind,
            value,
            fields,
        })
    }

    /// `(f32, f32)`, after its `(`: fields named by their position.
    fn parse_tuple_fields(&mut self) -> Option<Vec<Field>> {
        let types = self.parse_comma_list(TokenKind::CloseParen, |p| p.parse_type())?;
        self.expect(
            TokenKind::CloseParen,
            "a closing parenthesis after the fields",
        )?;
        let fields = (types.into_iter().enumerate())
            .map(|(i, ty)| Field {
                doc: None,
                visibility: Visibility::Public,
                name: Ident {
                    name: i.to_string().into(),
                    span: self.ast.types.span(ty).clone(),
                },
                ty,
            })
            .collect();
        Some(fields)
    }

    /// `{ w: f32, h: f32 }`, after its `{`. They're as visible as the enum.
    fn parse_variant_fields(&mut self) -> Option<Vec<Field>> {
        let fields = self.parse_comma_list(TokenKind::CloseBrace, |p| {
            let doc = p.doc_comment();
            let name = p.parse_ident("a field name")?;
            p.expect(TokenKind::Colon, "a field type")?;
            let ty = p.parse_type()?;
            Some(Field {
                doc,
                visibility: Visibility::Public,
                name,
                ty,
            })
        })?;
        self.expect(TokenKind::CloseBrace, "a closing brace after the fields")?;
        Some(fields)
    }

    /// `= 87`, `= -1`, at the `=`: the number a variant converts to.
    fn parse_variant_value(&mut self, kind: VariantKind) -> Option<Option<i128>> {
        let at = self.current_span();
        self.advance();
        let negative = self.eat(TokenKind::Minus);
        let n = self.parse_count("the variant's number")?;
        if kind != VariantKind::Unit {
            self.error_at(
                at.clone(),
                "a variant holding values has no number",
                "only variants that hold nothing convert to a number",
            );
        }
        match i128::try_from(n) {
            Ok(n) if negative => Some(Some(-n)),
            Ok(n) => Some(Some(n)),
            Err(_) => {
                self.error_at(at, "number is too large", "this does not fit in an `i128`");
                Some(None)
            }
        }
    }
}
