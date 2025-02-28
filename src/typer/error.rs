use miette::{diagnostic, LabeledSpan, MietteDiagnostic, Severity, SourceSpan};

use crate::ast::Typing;

pub fn new_mismatched_types(
    message: impl Into<String>,
    left_ty: &Typing<'_>,
    right_ty: &Typing<'_>,
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

pub fn mismatched_type(
    message: impl Into<String>,
    ty: &Typing<'_>,
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

pub(crate) fn undefined_identifier(
    message: impl Into<String>,
    identifier_name: &str,
    label: SourceSpan,
) -> MietteDiagnostic {
    let message = message.into();

    let label = LabeledSpan::new(
        format!("`{identifier_name}` is not defined in this scope").into(),
        label.offset(),
        label.len(),
    );

    diagnostic!(
        severity = Severity::Error,
        labels = vec![label],
        help = "try adding a declaration",
        "{message}"
    )
}
