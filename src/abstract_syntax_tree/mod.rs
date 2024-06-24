use std::collections::HashSet;

use crate::diagnostic::Range;

pub mod builder;

#[derive(Debug)]
pub enum AstractSyntax<'a> {
    Statement(Statement<'a>),
}

#[derive(Debug)]
pub enum Statement<'a> {
    EnumDeclaration(Spanned<'a, EnumDeclaration<'a>>),
}

#[derive(Debug)]
pub struct EnumDeclaration<'a> {
    pub identifier: Spanned<'a, &'a str>,
    pub items: Vec<Spanned<'a, &'a str>>,
}

#[derive(Debug)]
pub struct Spanned<'a, T> {
    pub value: &'a T,
    pub range: &'a Range<'a>,
}
