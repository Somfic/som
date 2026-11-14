use som_base::message::{Message, Part, code, func, struct_name, ty, var};
use som_base::{Diagnostic, Fix, Label, Span, msg};

#[derive(Debug, Clone)]
pub enum ResolveError {
    UnboundVariable {
        name: String,
        span: Span,
        help: Option<Message>,
        /// Changes that would fix it: the suggested name, or a `use`.
        fixes: Vec<Fix>,
    },
    UnknownFunction {
        name: String,
        span: Span,
        help: Option<Message>,
        /// Changes that would fix it: the suggested name, or a `use`.
        fixes: Vec<Fix>,
    },
    UnknownStruct {
        name: String,
        span: Span,
        help: Option<Message>,
        /// Changes that would fix it: the suggested name, or a `use`.
        fixes: Vec<Fix>,
    },
    UnknownType {
        name: String,
        span: Span,
        help: Option<Message>,
        /// Changes that would fix it: the suggested name, or a `use`.
        fixes: Vec<Fix>,
    },
    UnknownTrait {
        name: String,
        span: Span,
        help: Option<Message>,
        /// Changes that would fix it: the suggested name, or a `use`.
        fixes: Vec<Fix>,
    },
    DuplicateDefinition {
        name: String,
        span: Span,
        previous: Span,
    },
    /// `break x;` or `continue x;` where `x` isn't a `for` loop's variable.
    NotALoop {
        keyword: &'static str,
        name: String,
        span: Span,
        defined_at: Span,
    },
}

impl ResolveError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        type Parts<'a> = (&'a str, Part, &'a Span, &'a Option<Message>, &'a Vec<Fix>);
        let (what, name, span, help, fixes): Parts = match self {
            ResolveError::UnboundVariable {
                name,
                span,
                help,
                fixes,
            } => ("variable", var(name), span, help, fixes),
            ResolveError::UnknownFunction {
                name,
                span,
                help,
                fixes,
            } => ("function", func(name), span, help, fixes),
            ResolveError::UnknownStruct {
                name,
                span,
                help,
                fixes,
            } => ("struct", struct_name(name), span, help, fixes),
            ResolveError::UnknownType {
                name,
                span,
                help,
                fixes,
            } => ("type", ty(name), span, help, fixes),
            ResolveError::UnknownTrait {
                name,
                span,
                help,
                fixes,
            } => ("trait", ty(name), span, help, fixes),
            ResolveError::DuplicateDefinition {
                name,
                span,
                previous,
            } => {
                return Diagnostic::error(msg![var(name), " is defined more than once"])
                    .with_label(Label::primary(span.clone(), "redefined here"))
                    .with_label(Label::secondary(previous.clone(), "first defined here"));
            }
            ResolveError::NotALoop {
                keyword,
                name,
                span,
                defined_at,
            } => {
                return Diagnostic::error(msg![var(name), " isn't a loop"])
                    .with_label(Label::primary(
                        span.clone(),
                        msg![
                            code(*keyword),
                            " needs the variable of a ",
                            code("for"),
                            " around it"
                        ],
                    ))
                    .with_label(Label::secondary(defined_at.clone(), "defined here"));
            }
        };

        let mut diagnostic = Diagnostic::error(msg!["cannot find ", what, " ", name])
            .with_label(Label::primary(span.clone(), "not found in this scope"));
        if let Some(help) = help {
            diagnostic = diagnostic.with_hint(help.clone());
        }
        for fix in fixes {
            diagnostic = diagnostic.with_fix(fix.clone());
        }
        diagnostic
    }
}
