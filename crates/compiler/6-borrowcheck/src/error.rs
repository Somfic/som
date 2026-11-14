use som_base::message::code;
use som_base::{Diagnostic, Label, Span, msg};

#[derive(Debug, Clone)]
pub enum BorrowError {
    UseAfterMove {
        name: String,
        span: Span,
        moved_at: Span,
    },
    /// Moving a value out from behind a reference, which would leave the referent empty.
    MoveOutOfReference { name: String, span: Span },
    MoveWhileBorrowed {
        name: String,
        span: Span,
        borrowed_at: Span,
    },
    /// A borrow that conflicts with one that is still alive: two `&mut`, or `&` and `&mut`.
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
    /// A reference that outlives what it points to.
    DanglingReference {
        name: String,
        borrowed_at: Span,
        dies_at: Span,
    },
    /// A function that can return without assigning one of its `out` parameters.
    OutNotAssigned { name: String, span: Span },
    /// An `out` parameter read before the function assigned it.
    OutReadBeforeAssigned { name: String, span: Span },
    /// A reference to a temporary that's still used after the temporary is dropped, at the
    /// end of its statement: `let r = make().get();`.
    TemporaryDropped { borrowed_at: Span, dropped_at: Span },
    /// Moving a value out of one whose type implements `Drop`, which needs it to drop.
    MoveOutOfDrop {
        name: String,
        ty: String,
        span: Span,
    },
    /// A function that calls itself on every path, so it never returns.
    EndlessRecursion { name: String, span: Span },
}

fn kind(mutable: bool) -> &'static str {
    if mutable { "mutable" } else { "immutable" }
}

impl BorrowError {
    /// Names here are places like `p.x` or `*r`: som expressions, so they're shown as code.
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            BorrowError::UseAfterMove {
                name,
                span,
                moved_at,
            } => {
                let d = Diagnostic::error(msg!["use of moved value ", code(name)]);
                // The same code moving twice means a loop moved it on an earlier iteration.
                let d = if moved_at.start_offset == span.start_offset {
                    d.with_label(Label::primary(
                        span.clone(),
                        "value moved here, in a previous iteration of the loop",
                    ))
                } else {
                    d.with_label(Label::primary(span.clone(), "value used here after move"))
                        .with_label(Label::secondary(moved_at.clone(), "value moved here"))
                };
                d.with_hint(msg![
                    "consider borrowing with ",
                    code(format!("&{name}")),
                    " instead of moving it"
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
                name,
                borrowed_at,
                dies_at,
            } if name == "temporary value" => {
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
                name,
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
