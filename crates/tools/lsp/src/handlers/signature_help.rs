use som_ast::{Expr, Id};
use som_driver::Analysis;
use som_resolve::DefKind;
use tower_lsp::lsp_types::{
    ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation,
};

use super::Cursor;
use crate::docs::{array_method_doc, array_of, markdown, params_with, signature_with};
use crate::lookup::covers;
use crate::types::method_subst;

/// The innermost call around the cursor: the function it calls, and which argument the
/// cursor is in.
pub(crate) fn signature_help(a: &Analysis, cursor: &Cursor) -> Option<SignatureHelp> {
    let (call, expr) = a
        .ast
        .exprs
        .iter()
        .filter(|(id, e)| {
            matches!(e, Expr::Call { .. } | Expr::MethodCall { .. })
                && covers(a.ast.exprs.span(*id), &cursor.file, cursor.offset)
        })
        .min_by_key(|(id, _)| a.ast.exprs.span(*id).length)?;
    let active = active_parameter(a, cursor, call);
    let (label, params, doc) = described_call(a, call, expr)?;
    let parameters = params
        .into_iter()
        .map(|p| ParameterInformation {
            label: ParameterLabel::Simple(p),
            documentation: None,
        })
        .collect();
    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: markdown(doc),
            parameters: Some(parameters),
            active_parameter: Some(active),
        }],
        active_signature: Some(0),
        active_parameter: Some(active),
    })
}

/// The commas between the call's opening parenthesis and the cursor.
fn active_parameter(a: &Analysis, cursor: &Cursor, call: Id<Expr>) -> u32 {
    let span = a.ast.exprs.span(call);
    let before_cursor =
        &cursor.text[span.start_offset..cursor.offset.min(span.start_offset + span.length)];
    before_cursor.find('(').map_or(0, |open| {
        before_cursor[open + 1..].matches(',').count() as u32
    })
}

/// The called function's signature, its parameters after any `self`, and its documentation.
fn described_call(
    a: &Analysis,
    call: Id<Expr>,
    expr: &Expr,
) -> Option<(String, Vec<String>, Option<String>)> {
    if let Expr::MethodCall { receiver, .. } = expr
        && let Some(method) = a.typed.array_method(call)
        && let Some(sequence) = array_of(a, a.typed.expr(*receiver))
    {
        let doc = array_method_doc(a, method, sequence);
        return Some((doc.signature, doc.params, Some(doc.doc)));
    }
    let (def, skip_self) = match expr {
        Expr::Call { .. } => (a.typed.callee(call, &a.names), 0),
        _ => (a.typed.method(call), 1),
    };
    let Some(DefKind::Func(func)) = def.map(|d| a.names.def(d).kind.clone()) else {
        return None;
    };
    let subst = match expr {
        Expr::MethodCall { receiver, .. } => method_subst(a, func, a.typed.expr(*receiver)),
        _ => Vec::new(),
    };
    let params = params_with(a, func, &subst)
        .into_iter()
        .skip(skip_self)
        .collect();
    Some((
        signature_with(a, func, &subst),
        params,
        a.ast.funcs[func].doc.clone(),
    ))
}
