use miette::{diagnostic, LabeledSpan, MietteDiagnostic, Severity, SourceSpan};

use crate::ast::{
    Expression, FunctionDeclaration, GenericFunctionDeclaration, Identifier, Parameter,
    TypedExpression, TypedFunctionDeclaration, Typing,
};

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

pub fn mismatched_argument(
    message: impl Into<String>,
    argument: &TypedExpression<'_>,
    parameter: &Parameter<'_>,
    hint: impl Into<String>,
) -> Option<MietteDiagnostic> {
    let message = message.into();

    Some(diagnostic!(
        severity = Severity::Error,
        labels = vec![
            argument.label(format!("{}", argument.ty)),
            parameter.label(format!("{}", parameter.ty))
        ],
        help = hint.into(),
        "{message}"
    ))
}

pub fn unexpected_argument(
    message: impl Into<String>,
    function: &TypedFunctionDeclaration<'_>,
    argument: &TypedExpression<'_>,
    hint: impl Into<String>,
) -> Option<MietteDiagnostic> {
    let message = message.into();

    Some(diagnostic!(
        severity = Severity::Error,
        labels = vec![
            argument.label(format!("{}", argument.ty)),
            function.label("unexpected argument"),
        ],
        help = hint.into(),
        "{message}"
    ))
}

pub fn missing_argument(
    message: impl Into<String>,
    function_call: &Expression<'_>,
    parameter: &Parameter<'_>,
    hint: impl Into<String>,
) -> Option<MietteDiagnostic> {
    let message = message.into();

    Some(diagnostic!(
        severity = Severity::Error,
        labels = vec![
            function_call.label(format!("missing value for `{}`", parameter.identifier)),
            parameter.label(format!("`{}` paramater", parameter.identifier))
        ],
        help = hint.into(),
        "{message}"
    ))
}

pub(crate) fn undefined_type(
    message: impl Into<String>,
    identifier: &Identifier,
    label: SourceSpan,
) -> MietteDiagnostic {
    let message = message.into();

    let label = LabeledSpan::new(
        Some("this type could not be found".into()),
        label.offset(),
        label.len(),
    );

    diagnostic!(
        severity = Severity::Error,
        labels = vec![label],
        help = format!("define a type named `{}`", identifier),
        "{message}"
    )
}

pub(crate) fn undefined_variable(
    message: impl Into<String>,
    identifier: &Identifier,
    label: SourceSpan,
) -> Option<MietteDiagnostic> {
    let message = message.into();

    let label = LabeledSpan::new(
        Some("this variable could not be found".into()),
        label.offset(),
        label.len(),
    );

    Some(diagnostic!(
        severity = Severity::Error,
        labels = vec![label],
        help = format!("define a variable named `{}`", identifier),
        "{message}"
    ))
}

pub(crate) fn undefined_function(
    message: impl Into<String>,
    identifier: &Identifier,
    label: SourceSpan,
) -> Option<MietteDiagnostic> {
    let message = message.into();

    let label = LabeledSpan::new(
        Some("this function could not be found".into()),
        label.offset(),
        label.len(),
    );

    Some(diagnostic!(
        severity = Severity::Error,
        labels = vec![label],
        help = format!("define a function named `{}`", identifier),
        "{message}"
    ))
}
