use miette::{diagnostic, LabeledSpan, MietteDiagnostic, Severity, SourceSpan};

use crate::ast::{Expression, Typing};

pub fn new_mismatched_types(
    message: impl Into<String>,
    left_ty: &Typing<'_>,
    right_ty: &Typing<'_>,
    hint: impl Into<String>,
) -> Option<MietteDiagnostic> {
    let message = message.into();

    if left_ty.is_unknown() || right_ty.is_unknown() {
        return None;
    }

    Some(diagnostic!(
        severity = Severity::Error,
        labels = vec![
            left_ty.label(format!("{}", left_ty)),
            right_ty.label(format!("{}", right_ty)),
        ],
        help = hint.into(),
        "{message}"
    ))
}

pub fn mismatched_type(
    message: impl Into<String>,
    ty: &Typing<'_>,
    hint: impl Into<String>,
) -> Option<MietteDiagnostic> {
    let message = message.into();

    if ty.is_unknown() {
        return None;
    }

    Some(diagnostic!(
        severity = Severity::Error,
        labels = vec![ty.label(format!("{}", ty)),],
        help = hint.into(),
        "{message}"
    ))
}

pub fn mismatched_arguments(
    message: impl Into<String>,
    given_arguments: Vec<Expression>,
    expected_arguments: Vec<Typing>,
    hint: impl Into<String>,
) -> Option<MietteDiagnostic> {
    let message = message.into();

    let mut labels = Vec::new();
    for (i, argument) in given_arguments.iter().enumerate() {
        labels.push(argument.label(format!("argument {i}")));
    }
    for (i, argument) in expected_arguments.iter().enumerate() {
        labels.push(argument.label(format!("argument {i}")));
    }
    Some(diagnostic!(
        severity = Severity::Error,
        labels = labels,
        help = hint.into(),
        "{message}"
    ))
}

pub(crate) fn undefined_variable(
    message: impl Into<String>,
    identifier_name: &str,
    label: SourceSpan,
) -> Option<MietteDiagnostic> {
    let message = message.into();

    let label = LabeledSpan::new(
        format!("the variable `{identifier_name}` is not defined in this scope").into(),
        label.offset(),
        label.len(),
    );

    Some(diagnostic!(
        severity = Severity::Error,
        labels = vec![label],
        help = "create a variable with this name",
        "{message}"
    ))
}

pub(crate) fn undefined_function(
    message: impl Into<String>,
    identifier_name: &str,
    label: SourceSpan,
) -> Option<MietteDiagnostic> {
    let message = message.into();

    let label = LabeledSpan::new(
        format!("the function `{identifier_name}` is not defined in this scope").into(),
        label.offset(),
        label.len(),
    );

    Some(diagnostic!(
        severity = Severity::Error,
        labels = vec![label],
        help = "create a function with this name",
        "{message}"
    ))
}
