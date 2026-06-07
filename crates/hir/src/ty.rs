use som_common::{Span, expand_enum};

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug, Clone, Copy)]
    pub enum Type {
        Error,
        Int,
        Bool,
    } with { span: Span }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Type::Int { .. } => "i32",
            Type::Bool { .. } => "bool",
            Type::Error { .. } => "?",
        })
    }
}
