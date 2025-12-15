use crate::diagnostics::{Diagnostic, Label};
use crate::{ExprId, TypedAst};

#[derive(Debug, Clone)]
pub enum BorrowError {
    UseAfterMove {
        name: String,
        use_expr: ExprId,
        moved_at: ExprId,
    },
    MoveWhileBorrowed {
        name: String,
        move_expr: ExprId,
        borrow_expr: ExprId,
    },
    ConflictingBorrow {
        name: String,
        new_expr: ExprId,
        new_mut: bool,
        existing_expr: ExprId,
        existing_mut: bool,
    },
    UseWhileMutBorrowed {
        name: String,
        use_expr: ExprId,
        borrow_expr: ExprId,
    },
    DanglingReference {
        name: String,
        borrow_expr: ExprId,
        return_expr: ExprId,
    },
}

impl BorrowError {
    pub fn to_diagnostic(&self, typed_ast: &TypedAst) -> Diagnostic {
        match self {
            BorrowError::UseAfterMove {
                name,
                use_expr,
                moved_at,
            } => {
                let mut diag = Diagnostic::error(format!("use of moved value: `{}`", name));

                let use_span = typed_ast.ast.get_expr_span(use_expr);
                diag = diag.with_label(Label::primary(
                    use_span.clone(),
                    "value used here after move",
                ));

                let move_span = typed_ast.ast.get_expr_span(moved_at);
                diag = diag.with_label(Label::secondary(move_span.clone(), "value moved here"));

                diag.with_hint(format!(
                    "consider using `&{}` to borrow instead of moving",
                    name
                ))
            }

            BorrowError::MoveWhileBorrowed {
                name,
                move_expr,
                borrow_expr,
            } => {
                let mut diag =
                    Diagnostic::error(format!("cannot move `{}` because it is borrowed", name));

                let move_span = typed_ast.ast.get_expr_span(move_expr);
                diag = diag.with_label(Label::primary(move_span.clone(), "move occurs here"));

                let borrow_span = typed_ast.ast.get_expr_span(borrow_expr);
                diag = diag.with_label(Label::secondary(borrow_span.clone(), "borrow occurs here"));

                diag
            }

            BorrowError::ConflictingBorrow {
                name,
                new_expr,
                new_mut,
                existing_expr,
                existing_mut,
            } => {
                let msg = if *new_mut {
                    format!(
                        "cannot borrow `{}` as mutable because it is already borrowed as {}",
                        name,
                        if *existing_mut {
                            "mutable"
                        } else {
                            "immutable"
                        }
                    )
                } else {
                    format!(
                        "cannot borrow `{}` as immutable because it is already borrowed as mutable",
                        name
                    )
                };

                let mut diag = Diagnostic::error(msg);

                let new_span = typed_ast.ast.get_expr_span(new_expr);

                diag = diag.with_label(Label::primary(
                    new_span.clone(),
                    if *new_mut {
                        "mutable borrow occurs here"
                    } else {
                        "immutable borrow occurs here"
                    },
                ));

                let existing_span = typed_ast.ast.get_expr_span(existing_expr);
                diag = diag.with_label(Label::secondary(
                    existing_span.clone(),
                    if *existing_mut {
                        "mutable borrow occurs here"
                    } else {
                        "immutable borrow occurs here"
                    },
                ));

                diag
            }

            BorrowError::UseWhileMutBorrowed {
                name,
                use_expr,
                borrow_expr,
            } => {
                let mut diag = Diagnostic::error(format!(
                    "cannot use `{}` because it is mutably borrowed",
                    name
                ));

                let use_span = typed_ast.ast.get_expr_span(use_expr);
                diag = diag.with_label(Label::primary(use_span.clone(), "use occurs here"));

                let borrow_span = typed_ast.ast.get_expr_span(borrow_expr);
                diag = diag.with_label(Label::secondary(
                    borrow_span.clone(),
                    "mutable borrow occurs here",
                ));

                diag
            }

            BorrowError::DanglingReference {
                name,
                borrow_expr,
                return_expr,
            } => {
                let mut diag = Diagnostic::error(format!(
                    "cannot return reference to local variable `{}`",
                    name
                ));

                let return_span = typed_ast.ast.get_expr_span(return_expr);
                diag = diag.with_label(Label::primary(
                    return_span.clone(),
                    "returns a reference to a local variable",
                ));

                let borrow_span = typed_ast.ast.get_expr_span(borrow_expr);
                diag = diag.with_label(Label::secondary(
                    borrow_span.clone(),
                    format!("`{}` is borrowed here", name),
                ));

                diag.with_hint(format!(
                    "`{}` will be dropped when the function returns",
                    name
                ))
            }
        }
    }
}
