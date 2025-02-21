use miette::{diagnostic, MietteDiagnostic, Severity};

use crate::ast::Type;

pub fn new_mismatched_types(left_ty: &Type<'_>, right_ty: &Type<'_>) -> MietteDiagnostic {
    diagnostic!(
        severity = Severity::Error,
        labels = vec![
            left_ty.label("left side of the expression"),
            right_ty.label("right side of the expression")
        ],
        "Mismatched types",
    )
}
