use std::collections::HashSet;

use crate::diagnostic::Range;

pub mod builder;

pub enum Ast<'a> {
    Statement(Statement<'a>),
}
pub enum Statement<'a> {
    Empty,
    EnumDeclaration(Spanned<'a, EnumDeclaration<'a>>),
}

pub struct EnumDeclaration<'a> {
    pub identifier: Spanned<'a, &'a str>,
    pub items: Vec<Spanned<'a, &'a str>>,
}

pub struct Spanned<'a, T> {
    pub value: T,
    pub range: Range<'a>,
}
