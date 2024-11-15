use std::borrow::Cow;

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum Error<'de> {
    #[error("type error")]
    TypeError(TypeError<'de>),
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum TypeError<'de> {
    #[error("unknown identifier")]
    #[diagnostic(help("try doing this instead"))]
    UnknownIdentifier {
        identifier: Cow<'de, str>,

        #[label("this identifier is not in scope")]
        span: miette::SourceSpan,
    },
}
