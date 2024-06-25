use std::collections::HashSet;

use crate::diagnostic::Range;

pub mod builder;

#[derive(PartialEq, Clone, Debug)]
pub enum AstractSyntax<'a> {
    Statement(Statement<'a>),
}

#[derive(PartialEq, Clone, Debug)]
pub enum Statement<'a> {
    EnumDeclaration(Spanned<'a, EnumDeclaration<'a>>),
}

#[derive(PartialEq, Clone, Debug)]
pub struct EnumDeclaration<'a> {
    pub identifier: Spanned<'a, &'a str>,
    pub items: Vec<Spanned<'a, &'a str>>,
}

#[derive(PartialEq, Clone, Debug)]
pub struct Spanned<'a, T> {
    pub value: &'a T,
    pub range: &'a Range<'a>,
}
