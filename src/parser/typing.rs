use crate::{
    ast::Type,
    parser::{Parse, Parser},
    ParserError, Result,
};

impl Parse for Type {
    type Params = ();

    fn parse(input: &mut Parser, _params: Self::Params) -> Result<Self> {
        let peek = input.peek_expect("a type")?.clone();

        let Some(parse_function) = input.lookup.type_lookup.get(&peek.kind).cloned() else {
            return ParserError::ExpectedType
                .to_diagnostic()
                .with_label(peek.span.clone().label("expected this to be a type"))
                .with_hint(format!("{} cannot be parsed as a type", peek))
                .to_err();
        };

        parse_function(input)
    }
}
