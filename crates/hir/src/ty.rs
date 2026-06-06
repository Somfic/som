use som_common::{Span, expand_enum};

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug, Clone, Copy)]
    pub enum Type {
        Error,
        Int,
    } with { span: Span }
}
