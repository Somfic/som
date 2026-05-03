use som::Id;

use crate::{Expr, Parser};

type PrefixParser = for<'a> fn(&mut Parser<'a>) -> Id<Expr>;
type InfixParser = for<'a> fn(&mut Parser<'a>, Id<Expr>) -> Id<Expr>;

pub(crate) struct PrefixRule {
    pub parse: PrefixParser,
}

pub(crate) struct InfixRule {
    pub parse: InfixParser,
    pub l_bp: u8,
    pub r_bp: u8,
}

pub(crate) fn prefix(parser: PrefixParser) -> PrefixRule {
    PrefixRule { parse: parser }
}

pub(crate) fn infix(parser: InfixParser, l_bp: u8, r_bp: u8) -> InfixRule {
    InfixRule {
        parse: parser,
        l_bp,
        r_bp,
    }
}

pub(crate) fn postfix(parser: InfixParser, bp: u8) -> InfixRule {
    InfixRule {
        parse: parser,
        l_bp: bp,
        r_bp: 0,
    }
}
