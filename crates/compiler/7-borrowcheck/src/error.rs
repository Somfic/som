use som_base::message::code;
use som_base::{Diagnostic, Label, Span, msg};

#[derive(Debug, Clone)]
pub enum BorrowError {
    UseAfterMove {
        name: String,
        ty: String,
        span: Span,
        moved_at: Span,
    },
    MoveOutOfReference {
        name: String,
        span: Span,
    },
    MoveWhileBorrowed {
        name: String,
        span: Span,
        borrowed_at: Span,
    },
    ConflictingBorrow {
        name: String,
        span: Span,
        mutable: bool,
        existing_at: Span,
        existing_mutable: bool,
    },
    UseWhileMutBorrowed {
        name: String,
        span: Span,
        borrowed_at: Span,
    },
    AssignWhileBorrowed {
        name: String,
        span: Span,
        borrowed_at: Span,
    },
    DanglingReference {
        /// `None` for a temporary.
        name: Option<String>,
        borrowed_at: Span,
        dies_at: Span,
    },
    OutNotAssigned {
        name: String,
        span: Span,
    },
    OutReadBeforeAssigned {
        name: String,
        span: Span,
    },
    /// Dropped at the end of its statement: `let r = make().get();`.
    TemporaryDropped {
        borrowed_at: Span,
        dropped_at: Span,
    },
    MoveOutOfDrop {
        name: String,
        ty: String,
        span: Span,
    },
    EndlessRecursion {
        name: String,
        span: Span,
    },
}

fn kind(mutable: bool) -> &'static str {
    if mutable { "mutable" } else { "immutable" }
}

impl BorrowError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            BorrowError::UseAfterMove {
                name,
                ty,
                span,
                moved_at,
            } => {
                let d = Diagnostic::error(msg![code(name), " is used after it was consumed"]);
                // The same code moving it twice: a loop, which moved it the time before, or
                // one expression using it twice, `[a, a]`.
                let d = if moved_at.start_offset == span.start_offset {
                    d.with_label(Label::primary(
                        span.clone(),
                        msg![
                            "consumed here more than once: by a loop going round again, ",
                            "or by using it twice here"
                        ],
                    ))
                } else {
                    d.with_label(Label::primary(
                        span.clone(),
                        "tried to use here, but it's gone",
                    ))
                    .with_label(Label::secondary(
                        moved_at.clone(),
                        "consumed here, gone after this point",
                    ))
                };
                d.with_hint(msg![
                    code(name),
                    " is a ",
                    code(ty),
                    ", which moves rather than copies: once it's consumed, it's gone"
                ])
                .with_hint(
                    "consume it only after its other uses, or only in the branch that needs it",
                )
                .with_hint(msg![
                    "or, if a reference is enough where it's consumed, borrow it there: ",
                    code(format!("&{name}"))
                ])
            }
            BorrowError::MoveOutOfReference { name, span } => {
                // What `*r` is moved out of is already behind the reference `r`.
                let borrowed = match name.strip_prefix('*') {
                    Some(reference) => reference.to_string(),
                    None => format!("&{name}"),
                };
                Diagnostic::error(msg!["cannot move ", code(name), " out of a reference"])
                    .with_label(Label::primary(span.clone(), "moved here"))
                    .with_hint(msg!["borrow it instead, with ", code(borrowed)])
            }
            BorrowError::MoveWhileBorrowed {
                name,
                span,
                borrowed_at,
            } => Diagnostic::error(msg!["cannot move ", code(name), " because it is borrowed"])
                .with_label(Label::primary(span.clone(), "moved here"))
                .with_label(Label::secondary(borrowed_at.clone(), "borrowed here")),
            BorrowError::ConflictingBorrow {
                name,
                span,
                mutable,
                existing_at,
                existing_mutable,
            } => Diagnostic::error(msg![
                "cannot borrow ",
                code(name),
                format!(
                    " as {} because it is also borrowed as {}",
                    kind(*mutable),
                    kind(*existing_mutable)
                ),
            ])
            .with_label(Label::primary(
                span.clone(),
                format!("{} borrow here", kind(*mutable)),
            ))
            .with_label(Label::secondary(
                existing_at.clone(),
                format!("{} borrow here", kind(*existing_mutable)),
            )),
            BorrowError::UseWhileMutBorrowed {
                name,
                span,
                borrowed_at,
            } => Diagnostic::error(msg![
                "cannot use ",
                code(name),
                " because it is mutably borrowed"
            ])
            .with_label(Label::primary(span.clone(), "used here"))
            .with_label(Label::secondary(borrowed_at.clone(), "mutable borrow here")),
            BorrowError::AssignWhileBorrowed {
                name,
                span,
                borrowed_at,
            } => Diagnostic::error(msg![
                "cannot assign to ",
                code(name),
                " because it is borrowed"
            ])
            .with_label(Label::primary(span.clone(), "assigned here"))
            .with_label(Label::secondary(borrowed_at.clone(), "borrowed here")),
            BorrowError::DanglingReference {
                name: None,
                borrowed_at,
                dies_at,
            } => {
                // No name to give it: a value made for the expression, like a collection
                // literal's elements behind a slice.
                Diagnostic::error("reference to a temporary value outlives it")
                    .with_label(Label::primary(
                        borrowed_at.clone(),
                        "this value is temporary",
                    ))
                    .with_label(Label::secondary(
                        dies_at.last_char(),
                        "it stops existing at the end of this block",
                    ))
            }
            BorrowError::DanglingReference {
                name: Some(name),
                borrowed_at,
                dies_at,
            } => Diagnostic::error(msg!["reference to ", code(name), " outlives it"])
                .with_label(Label::primary(borrowed_at.clone(), "borrowed here"))
                .with_label(Label::secondary(
                    dies_at.last_char(),
                    msg![code(name), " stops existing here"],
                )),
            BorrowError::OutNotAssigned { name, span } => Diagnostic::error(msg![
                code("out"),
                " parameter ",
                code(name),
                " isn't assigned on every path"
            ])
            .with_label(Label::primary(
                span.clone(),
                "the function can return without assigning this",
            )),
            BorrowError::OutReadBeforeAssigned { name, span } => Diagnostic::error(msg![
                code("out"),
                " parameter ",
                code(name),
                " is read before it's assigned"
            ])
            .with_label(Label::primary(span.clone(), "read here"))
            .with_hint("callers pass a fresh variable, so it has no value until it's assigned"),
            BorrowError::TemporaryDropped {
                borrowed_at,
                dropped_at,
            } => Diagnostic::error("temporary value dropped while still borrowed")
                .with_label(Label::primary(
                    borrowed_at.clone(),
                    "this value is temporary",
                ))
                .with_label(Label::secondary(
                    dropped_at.last_char(),
                    "it's dropped at the end of this statement",
                ))
                .with_hint("give it a variable of its own first, so it lives until that ends"),
            BorrowError::MoveOutOfDrop { name, ty, span } => {
                Diagnostic::error(msg!["cannot move out of ", code(name)])
                    .with_label(Label::primary(
                        span.clone(),
                        msg![
                            code(ty.clone()),
                            " implements ",
                            code("Drop"),
                            ", which needs it"
                        ],
                    ))
                    .with_hint(msg!["borrow it instead: ", code(format!("&{name}"))])
            }
            BorrowError::EndlessRecursion { name: _, span } => {
                Diagnostic::error(msg!["infinite recursion"])
                    .with_label(Label::primary(
                        span.clone(),
                        "every path gets here before it can return",
                    ))
                    .with_hint("it never returns: give it a way to finish without this call")
            }
        }
    }
}
