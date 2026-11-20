use som_ast::{Expr, Id, Ident, Span, Where};

use crate::Parser;

/// Where the parser is, which changes how some tokens read.
#[derive(Default)]
pub(crate) struct Context {
    /// In the condition of an `if` or `while`, where `name {` starts the body, not a
    /// constructor. Cleared again inside parentheses and blocks.
    pub condition: bool,
    /// In an `@if`'s condition, where a type can be compared with another: `T == f32`.
    pub settled: bool,
    /// In an `impl` or `trait` block, where methods can take `self`.
    pub impl_block: bool,
    /// In a function's parameter list, where `->` after a parameter means its `)` is missing.
    pub params: bool,
    /// In what an `impl` is for, where a slice's elements can be named bare, `[T]`:
    /// `impl Index<usize> for [T]`, whose `&self` is the slice.
    pub impl_target: bool,
    /// How many bodies written without braces (`then … end`, Python's `:`) this is in, where
    /// a line's missing `;` is noted in `foreign_semicolons`, for the body's fix.
    pub foreign_depth: usize,
    pub foreign_semicolons: Vec<Span>,
}

/// `@where` conditions, `@on` events and `@test` read, for the function after them, with the
/// `///` comment above them and where they start.
#[derive(Default)]
pub(crate) struct PendingWheres {
    pub conditions: Vec<Where>,
    pub listens: Vec<Ident>,
    pub test: bool,
    /// `@vertex` or `@fragment`.
    pub stage: Option<som_lang::Stage>,
    pub doc: Option<String>,
    pub start: Option<Span>,
}

impl PendingWheres {
    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty() && self.listens.is_empty() && !self.test && self.stage.is_none()
    }
}

/// The `@if`s among an impl block's members the parser is inside: what the members after
/// them need to hold.
#[derive(Default)]
pub(crate) struct MemberConditions {
    pub conditions: Vec<Id<Expr>>,
    /// For each `@if`: how many of `conditions` it added (`else @if` adds the earlier
    /// conditions, not holding), and whether the last of them is its own condition, which an
    /// `else` after it turns around.
    pub frames: Vec<(usize, bool)>,
}

impl Parser<'_> {
    /// Runs `f` with the flag `flag` picks set to `value`, then puts it back.
    pub(crate) fn with_flag<T>(
        &mut self,
        flag: fn(&mut Context) -> &mut bool,
        value: bool,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let outer = std::mem::replace(flag(&mut self.ctx), value);
        let result = f(self);
        *flag(&mut self.ctx) = outer;
        result
    }

    pub(crate) fn with_condition<T>(
        &mut self,
        in_condition: bool,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.with_flag(|c| &mut c.condition, in_condition, f)
    }

    /// Outside any condition: inside parentheses, brackets and blocks.
    pub(crate) fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.with_condition(false, f)
    }

    /// Runs `f` inside a body written without braces.
    pub(crate) fn in_foreign_body<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.ctx.foreign_depth += 1;
        let result = f(self);
        self.ctx.foreign_depth -= 1;
        result
    }
}
