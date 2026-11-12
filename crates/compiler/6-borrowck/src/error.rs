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
                Diagnostic::error(msg!["cannot move ", code(name), " out of a reference"])
                    .with_label(Label::primary(span.clone(), "moved here"))
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
            } => Diagnostic::error(msg!["reference to ", code(name), " outlives it"])
                .with_label(Label::primary(borrowed_at.clone(), "borrowed here"))
                .with_label(Label::secondary(
                    dies_at.last_char(),
                    msg![code(name), " stops existing here"],
                )),
        }
    }
}
