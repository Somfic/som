use std::collections::HashMap;

use som_ast::{BinOp, Expr, Id, Span, Stmt, TypeExpr};
use som_driver::Analysis;
use som_resolve::{DefId, DefKind};
use som_typeck::{Ty, TypeId};
use tower_lsp::lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintTooltip, Position, Range, TextEdit,
};

use crate::convert;
use crate::docs::ty;
use crate::types::is_self;

pub(crate) fn inlay_hints(a: &Analysis, file: &str) -> Vec<InlayHint> {
    let mut hints = variable_type_hints(a, file);
    hints.extend(return_type_hints(a, file));
    hints.extend(annotation_hints(a, file));
    hints.extend(optional_condition_hints(a, file));
    hints.extend(out_type_hints(a, file));
    // Before the conversions, which go at the same place.
    hints.extend(parameter_hints(a, file));
    hints.extend(conversion_hints(a, file));
    hints.extend(constant_call_hints(a, file));
    hints
}

fn hint(position: Position, label: String) -> InlayHint {
    InlayHint {
        position,
        label: InlayHintLabel::String(label),
        kind: None,
        text_edits: None,
        tooltip: None,
        padding_left: None,
        padding_right: None,
        data: None,
    }
}

/// `: i32` after the name a variable is declared with.
fn type_hint(name: &Span, ty: String) -> InlayHint {
    InlayHint {
        kind: Some(InlayHintKind::TYPE),
        ..hint(convert::span_to_range(name).end, format!(": {ty}"))
    }
}

/// The inferred type after every unannotated `let`, and every `for` loop's name.
fn variable_type_hints(a: &Analysis, file: &str) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    for (id, stmt) in a.ast.stmts.iter() {
        let name = match stmt {
            Stmt::Let { name, ty: None, .. } | Stmt::For { name, .. } => name,
            _ => continue,
        };
        if name.span.source.identifier() != file {
            continue;
        }
        let Some(t) = a.names.local(id).and_then(|def| a.typed.try_local(def)) else {
            continue;
        };
        if matches!(a.typed.ty(t), Ty::Unit | Ty::Error) {
            continue;
        }
        hints.push(type_hint(&name.span, ty(a, t)));
    }
    hints
}

/// `if x` on an optional is `if x != none`, and so is each optional operand of `&&`, `||`
/// and `!`.
fn optional_condition_hints(a: &Analysis, file: &str) -> Vec<InlayHint> {
    let mut conditions: Vec<Id<Expr>> = a
        .ast
        .stmts
        .iter()
        .filter_map(|(_, stmt)| match stmt {
            Stmt::If { condition, .. } => Some(*condition),
            _ => None,
        })
        .collect();
    for (_, expr) in a.ast.exprs.iter() {
        match expr {
            Expr::Binary {
                op: BinOp::And | BinOp::Or,
                lhs,
                rhs,
            } => conditions.extend([*lhs, *rhs]),
            Expr::Not(inner) => conditions.push(*inner),
            _ => {}
        }
    }
    conditions.sort_by_key(|e| e.id);
    conditions.dedup();
    let mut hints = Vec::new();
    for condition in conditions {
        let span = a.ast.exprs.span(condition);
        if span.source.identifier() != file
            || !matches!(a.typed.ty(a.typed.expr(condition)), Ty::Optional(_))
        {
            continue;
        }
        hints.push(InlayHint {
            tooltip: Some(InlayHintTooltip::String(
                "`if` on an optional checks whether it holds a value".into(),
            )),
            padding_left: Some(true),
            ..hint(convert::span_to_range(span).end, " != none".into())
        });
    }
    hints
}

/// The type the parameter gives every `out name` argument: `out db: Sqlite3`.
fn out_type_hints(a: &Analysis, file: &str) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    for (id, expr) in a.ast.exprs.iter() {
        let Expr::Out(name) = expr else { continue };
        if name.span.source.identifier() != file {
            continue;
        }
        let Some(t) = a.names.expr(id).and_then(|def| a.typed.try_local(def)) else {
            continue;
        };
        if !matches!(a.typed.ty(t), Ty::Error) {
            hints.push(type_hint(&name.span, ty(a, t)));
        }
    }
    hints
}

/// The parameter each argument is for: `init_window(width: 800, …)`.
fn parameter_hints(a: &Analysis, file: &str) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    for (id, expr) in a.ast.exprs.iter() {
        let (def, args, skip) = match expr {
            Expr::Call { args, .. } => (a.typed.callee(id, &a.names), args, 0),
            // A method's first parameter is the receiver, before the dot.
            Expr::MethodCall { args, .. } => (a.typed.method(id), args, 1),
            _ => continue,
        };
        let Some(DefKind::Func(f)) = def.map(|d| a.names.def(d).kind.clone()) else {
            continue;
        };
        for (&arg, param) in args.iter().zip(a.ast.funcs[f].params.iter().skip(skip)) {
            let span = a.ast.exprs.span(arg);
            if span.source.identifier() != file || !worth_naming(a, arg, &param.name.name) {
                continue;
            }
            hints.push(InlayHint {
                kind: Some(InlayHintKind::PARAMETER),
                padding_right: Some(true),
                ..hint(
                    convert::span_to_range(span).start,
                    format!("{}:", param.name.name),
                )
            });
        }
    }
    hints
}

/// Whether an argument's parameter name says something: not when the argument is a
/// variable of that name already, or the name was made up because the C header had none
/// (`arg0`).
fn worth_naming(a: &Analysis, arg: Id<Expr>, param: &str) -> bool {
    let made_up = param
        .strip_prefix("arg")
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    let same = match &a.ast.exprs[arg] {
        Expr::Var(path) => *path.name().name == *param,
        Expr::Out(name) => *name.name == *param,
        _ => false,
    };
    !made_up && !same
}

/// A value converted through `From`, which the source doesn't show: `List::from([1, 2])`.
fn conversion_hints(a: &Analysis, file: &str) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    for (id, _) in a.ast.exprs.iter() {
        let Some((_, &[target, source, ..])) = a.typed.conversion(id) else {
            continue;
        };
        let span = a.ast.exprs.span(id);
        if span.source.identifier() != file {
            continue;
        }
        let Ty::Struct(def, _) = a.typed.ty(target) else {
            continue;
        };
        let tooltip = format!(
            "converted to `{}` by its `From<{}>`",
            ty(a, target),
            ty(a, source)
        );
        let range = convert::span_to_range(span);
        for (position, label) in [
            (range.start, format!("{}::from(", a.names.def(*def).name)),
            (range.end, ")".to_string()),
        ] {
            hints.push(InlayHint {
                tooltip: Some(InlayHintTooltip::String(tooltip.clone())),
                ..hint(position, label)
            });
        }
    }
    hints
}

/// What a call that could be worked out while compiling gives: `count_primes(100) = 25`.
fn constant_call_hints(a: &Analysis, file: &str) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    for warning in &a.warnings {
        let som_driver::Warning::CouldBeConst(call) = warning else {
            continue;
        };
        let Some(value) = &call.value else {
            continue;
        };
        if call.span.source.identifier() != file {
            continue;
        }
        hints.push(InlayHint {
            tooltip: Some(InlayHintTooltip::String(format!(
                "`{}` is {value} every time: `{}` works it out while compiling",
                call.call, call.fix
            ))),
            padding_left: Some(true),
            ..hint(
                convert::span_to_range(&call.span).end,
                format!(" = {value}"),
            )
        });
    }
    hints
}

/// `let list: List = [1, 2]`: the type arguments each `let` annotation in `file` leaves out,
/// after it.
fn annotation_hints(a: &Analysis, file: &str) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    for (id, stmt) in a.ast.stmts.iter() {
        let Stmt::Let {
            ty: Some(annotation),
            ..
        } = stmt
        else {
            continue;
        };
        if a.ast.types.span(*annotation).source.identifier() != file {
            continue;
        }
        if let Some(t) = a.names.local(id).and_then(|def| a.typed.try_local(def)) {
            omitted_args_hints(a, *annotation, t, &mut hints);
        }
    }
    hints
}

/// `<i32>` after each generic struct in annotation `written` whose type arguments are left
/// out, from `t`, the type it turned out to be.
fn omitted_args_hints(a: &Analysis, written: Id<TypeExpr>, t: TypeId, hints: &mut Vec<InlayHint>) {
    let type_args = |shown: Vec<String>| InlayHint {
        kind: Some(InlayHintKind::TYPE),
        ..hint(
            convert::span_to_range(a.ast.types.span(written)).end,
            format!("<{}>", shown.join(", ")),
        )
    };
    // An alias: what its own parameters turned out to be, not those of what it stands for.
    if let TypeExpr::Named { args, .. } = &a.ast.types[written]
        && let Some(alias) = a.names.ty(written)
        && let DefKind::Alias { ty: body } = a.names.def(alias).kind
    {
        let params = a.names.alias_params(alias);
        let mut bound = HashMap::new();
        alias_params_in(a, body, t, &mut bound);
        if args.is_empty() && !params.is_empty() {
            let shown: Vec<String> = (params.iter())
                .map(|p| bound.get(p).map_or("_".into(), |&arg| ty(a, arg)))
                .collect();
            hints.push(type_args(shown));
        }
        for (&arg, param) in args.iter().zip(params) {
            if let Some(&actual) = bound.get(param) {
                omitted_args_hints(a, arg, actual, hints);
            }
        }
        return;
    }
    match (&a.ast.types[written], a.typed.ty(t)) {
        (TypeExpr::Named { args, .. }, Ty::Struct(_, actual)) if args.is_empty() => {
            if !actual.is_empty() {
                hints.push(type_args(actual.iter().map(|&arg| ty(a, arg)).collect()));
            }
        }
        (TypeExpr::Named { args, .. }, Ty::Struct(_, actual)) => {
            for (&arg, &actual) in args.iter().zip(actual) {
                omitted_args_hints(a, arg, actual, hints);
            }
        }
        (TypeExpr::Optional(inner), Ty::Optional(actual))
        | (TypeExpr::Reference { to: inner, .. }, Ty::Ref { to: actual, .. })
        | (
            TypeExpr::Array { element: inner, .. },
            Ty::Array {
                element: actual, ..
            },
        ) => {
            omitted_args_hints(a, *inner, *actual, hints);
        }
        _ => {}
    }
}

/// What each alias parameter in `written`, an alias's type, stands for, given that it
/// turned out to be `t`.
fn alias_params_in(
    a: &Analysis,
    written: Id<TypeExpr>,
    t: TypeId,
    bound: &mut HashMap<DefId, TypeId>,
) {
    match (&a.ast.types[written], a.typed.ty(t)) {
        (TypeExpr::Named { args, .. }, actual) => match a.names.ty(written) {
            Some(def) if matches!(a.names.def(def).kind, DefKind::AliasParam { .. }) => {
                bound.insert(def, t);
            }
            // Another alias: what its parameters stand for, then what was written for them.
            Some(def) if let DefKind::Alias { ty: body } = a.names.def(def).kind => {
                let mut inner = HashMap::new();
                alias_params_in(a, body, t, &mut inner);
                for (&arg, param) in args.iter().zip(a.names.alias_params(def)) {
                    if let Some(&actual) = inner.get(param) {
                        alias_params_in(a, arg, actual, bound);
                    }
                }
            }
            _ => {
                if let Ty::Struct(_, actual) = actual {
                    for (&arg, &actual) in args.iter().zip(actual) {
                        alias_params_in(a, arg, actual, bound);
                    }
                }
            }
        },
        (TypeExpr::Array { element, len }, Ty::Array { element: e, len: l }) => {
            alias_params_in(a, *element, *e, bound);
            alias_params_in(a, *len, *l, bound);
        }
        (TypeExpr::Optional(inner), Ty::Optional(actual))
        | (TypeExpr::Reference { to: inner, .. }, Ty::Ref { to: actual, .. })
        | (TypeExpr::PointerTo(inner), Ty::Ptr(actual)) => {
            alias_params_in(a, *inner, *actual, bound);
        }
        _ => {}
    }
}

/// The inferred return type of each function, method or property in `file` that doesn't
/// write one, right after its parameters: `fn double(x: i32) -> i32`. Double-clicking writes
/// it in.
fn return_type_hints(a: &Analysis, file: &str) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    for (id, func) in a.ast.funcs.iter() {
        // `prop xy: Vector = …`: the type arguments its value gave, `<2, T>`, as for a `let`.
        if func.prop
            && let Some(written) = func.returns
            && a.ast.types.span(written).source.identifier() == file
        {
            omitted_args_hints(a, written, a.typed.func(id).ret, &mut hints);
        }
        let (None, Some(body)) = (func.returns, func.body) else {
            continue;
        };
        let at = a.ast.exprs.span(body);
        if at.source.identifier() != file {
            continue;
        }
        let ret = a.typed.func(id).ret;
        if matches!(a.typed.ty(ret), Ty::Unit | Ty::Error) {
            continue;
        }
        // With no return type written, only space, or the `=` of `fn f(x: i32) = value;`,
        // comes between the parameters' `)`, or a property's name, and the body.
        let mut before = at.source.content()[..at.start_offset].trim_end();
        if let Some(rest) = before.strip_suffix('=') {
            before = rest.trim_end();
        }
        if !before.ends_with(')') && !func.prop {
            continue;
        }
        let close = Span::from_range(before.len()..before.len(), at.source.clone());
        let position = convert::span_to_range(&close).start;
        let shown = match is_self(a, id, ret) {
            true => "Self".to_string(),
            false => ty(a, ret),
        };
        // A property's type goes after a `:`, as a field's does.
        let text = match func.prop {
            true => format!(": {shown}"),
            false => format!(" -> {shown}"),
        };
        hints.push(InlayHint {
            kind: Some(InlayHintKind::TYPE),
            text_edits: Some(vec![TextEdit {
                range: Range::new(position, position),
                new_text: text.clone(),
            }]),
            ..hint(position, text)
        });
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(hints: Vec<InlayHint>) -> Vec<(u32, u32, String)> {
        hints
            .into_iter()
            .map(|h| match h.label {
                InlayHintLabel::String(label) => (h.position.line, h.position.character, label),
                InlayHintLabel::LabelParts(_) => unreachable!(),
            })
            .collect()
    }

    #[test]
    fn inferred_return_types_are_hinted_after_the_parameters() {
        let dir = std::env::temp_dir().join(format!("som_lsp_return_hints_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main = dir.join("main.som");
        let text = "struct Rect { w: i32 }\n\
                    impl Rect {\n    prop area { self.w * 2 }\n}\n\
                    fn double(x: i32) = x * 2;\n\
                    fn written(x: i32) -> i32 { x }\n\
                    fn nothing() { }\n\
                    impl Rect {\n    prop half = self.w / 2;\n    prop pair: Pair = Pair { a: self.w };\n}\n\
                    struct Pair<T> { a: T }\n\
                    impl<T> Pair<T> {\n    fn of(a: T) { Pair { a: a } }\n    fn other(self) { Pair { a: 1 } }\n}\n\
                    fn main() -> i32 { double(Rect { w: 1 }.area) }\n";
        std::fs::write(&main, text).unwrap();
        let analysis = som_driver::analyze(&dir, &som_driver::Options::default());
        let file = main.to_string_lossy();
        let labels = labels(return_type_hints(&analysis, &file));
        std::fs::remove_dir_all(&dir).unwrap();
        // Not `written`, which says it, nor `nothing`, which gives nothing back.
        assert_eq!(
            labels,
            [
                (2, 13, ": i32".to_string()),
                (4, 17, " -> i32".to_string()),
                (8, 13, ": i32".to_string()),
                (9, 19, "<i32>".to_string()),
                (13, 15, " -> Self".to_string()),
                (14, 18, " -> Pair<i32>".to_string())
            ]
        );
    }

    #[test]
    fn aliases_hint_only_their_own_omitted_arguments() {
        let dir = std::env::temp_dir().join(format!("som_lsp_alias_hints_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main = dir.join("main.som");
        let text = "struct Vector<N: usize, T> { values: [T; N] }\n\
                    type Vector3f = Vector<3, f32>;\n\
                    type Vector3<T> = Vector<3, T>;\n\
                    struct Box<T> { value: T }\n\
                    fn main() -> i32 {\n\
                    let a: Vector3f = Vector { values: [0.0, 0.0, 0.0] };\n\
                    let b: Vector3 = Vector { values: [1, 2, 3] };\n\
                    let c: Box<Vector3f> = Box { value: a };\n\
                    let d: Vector = Vector { values: [true] };\n\
                    0\n}\n";
        std::fs::write(&main, text).unwrap();
        let analysis = som_driver::analyze(&dir, &som_driver::Options::default());
        let file = main.to_string_lossy();
        let labels = labels(annotation_hints(&analysis, &file));
        std::fs::remove_dir_all(&dir).unwrap();
        // Nothing for `Vector3f`, which leaves nothing out, nor inside `Box<Vector3f>`;
        // `Vector3`'s own parameter, not `Vector`'s two.
        assert_eq!(
            labels,
            [
                (6, 14, "<i32>".to_string()),
                (8, 13, "<1, bool>".to_string())
            ]
        );
    }

    #[test]
    fn parameter_hints_name_only_what_needs_naming() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.som");
        let source = "fn area(width: i32, height: i32, arg0: i32) -> i32 { width * height }\n\
                      fn main() { let width = 2; area(width, 3, 4); }\n";
        std::fs::write(&main, source).unwrap();
        let a = som_driver::analyze(&main, &Default::default());
        let named: Vec<&str> = a
            .ast
            .exprs
            .iter()
            .filter_map(|(_, e)| match e {
                Expr::Call { args, .. } => Some(args),
                _ => None,
            })
            .flatten()
            .zip(["width", "height", "arg0"])
            .filter(|(arg, param)| worth_naming(&a, **arg, param))
            .map(|(_, param)| param)
            .collect();
        assert_eq!(named, ["height"]);
    }

    #[test]
    fn out_arguments_are_not_named_again() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.som");
        let source = "fn divide(a: i32, b: i32, out remainder: i32) -> i32 {\n    \
                      remainder = a % b;\n    a / b\n}\n\n\
                      fn main() { let q = divide(17, 5, out remainder); }\n";
        std::fs::write(&main, source).unwrap();
        let a = som_driver::analyze(&main, &Default::default());
        let out = a.ast.exprs.iter().find(|(_, e)| matches!(e, Expr::Out(_)));
        assert!(!worth_naming(&a, out.unwrap().0, "remainder"));
    }
}
