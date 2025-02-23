use miette::{diagnostic, MietteDiagnostic, Severity};

use crate::ast::Type;

pub fn new_mismatched_types(
    message: impl Into<String>,
    left_ty: &Type<'_>,
    right_ty: &Type<'_>,
    hint: impl Into<String>,
) -> MietteDiagnostic {
    let message = message.into();

    diagnostic!(
        severity = Severity::Error,
        labels = vec![
            left_ty.label(format!("{}", left_ty)),
            right_ty.label(format!("{}", right_ty)),
        ],
        help = hint.into(),
        "{message}"
    )
}

pub fn new_mismatched_type(
    message: impl Into<String>,
    ty: &Type<'_>,
    hint: impl Into<String>,
) -> MietteDiagnostic {
    let message = message.into();

    diagnostic!(
        severity = Severity::Error,
        labels = vec![ty.label(format!("{}", ty)),],
        help = hint.into(),
        "{message}"
    )
}
