//! Types and patterns as they would be written in som.

use crate::*;

impl Ast {
    /// A type annotation as it would be written in som.
    pub fn type_text(&self, id: Id<TypeExpr>) -> String {
        match &self.types[id] {
            TypeExpr::Unit => "()".into(),
            TypeExpr::Primitive(p) => p.name().to_string(),
            TypeExpr::Named { path, args } if args.is_empty() => path.to_string(),
            TypeExpr::Named { path, args } => {
                let args: Vec<String> = args.iter().map(|&a| self.type_text(a)).collect();
                format!("{path}<{}>", args.join(", "))
            }
            TypeExpr::Dyn { path, args } if args.is_empty() => format!("dyn {path}"),
            TypeExpr::Dyn { path, args } => {
                let args: Vec<String> = args.iter().map(|&a| self.type_text(a)).collect();
                format!("dyn {path}<{}>", args.join(", "))
            }
            TypeExpr::Pointer => "*".into(),
            TypeExpr::PointerTo(to) => format!("*{}", self.type_text(*to)),
            TypeExpr::Shared(to) => format!("#{}", self.type_text(*to)),
            TypeExpr::Elements(element) => format!("[{}]", self.type_text(*element)),
            TypeExpr::Reference {
                mutable,
                lifetime,
                to,
            } => {
                let lifetime = lifetime
                    .as_ref()
                    .map(|l| format!("'{l} "))
                    .unwrap_or_default();
                let mutability = if *mutable { "mut " } else { "" };
                // `&i32?` is an optional reference.
                let to = match &self.types[*to] {
                    TypeExpr::Optional(_) => format!("({})", self.type_text(*to)),
                    _ => self.type_text(*to),
                };
                format!("&{lifetime}{mutability}{to}")
            }
            TypeExpr::Array { element, len } => {
                format!("[{}; {}]", self.type_text(*element), self.type_text(*len))
            }
            TypeExpr::Int(n) => n.to_string(),
            TypeExpr::Bool(b) => b.to_string(),
            TypeExpr::Binary { op, lhs, rhs } => {
                // A sum inside a product keeps its parentheses.
                let side = |id: Id<TypeExpr>| match (&self.types[id], op) {
                    (TypeExpr::Binary { op: BinOp::Add, .. }, BinOp::Multiply) => {
                        format!("({})", self.type_text(id))
                    }
                    _ => self.type_text(id),
                };
                format!("{} {} {}", side(*lhs), op.symbol(), side(*rhs))
            }
            TypeExpr::Slice { element, mutable } => {
                let mutability = if *mutable { "mut " } else { "" };
                format!("&{mutability}[{}]", self.type_text(*element))
            }
            // `fn() -> i32?` returns an optional.
            TypeExpr::Optional(inner) => match &self.types[*inner] {
                TypeExpr::Fn { .. } => format!("({})?", self.type_text(*inner)),
                _ => format!("{}?", self.type_text(*inner)),
            },
            TypeExpr::Failable { ok, errors } => {
                let ok = match &self.types[*ok] {
                    TypeExpr::Unit => String::new(),
                    TypeExpr::Fn { .. } => format!("({})", self.type_text(*ok)),
                    _ => self.type_text(*ok),
                };
                let errors = match errors.as_deref() {
                    None => String::new(),
                    Some([one]) => self.type_text(*one),
                    Some(many) => {
                        let many: Vec<String> = many.iter().map(|&e| self.type_text(e)).collect();
                        format!("({})", many.join(", "))
                    }
                };
                format!("{ok}!{errors}")
            }
            TypeExpr::Fn { params, returns } => {
                let params: Vec<String> = params.iter().map(|&p| self.type_text(p)).collect();
                let returns = returns
                    .map(|r| format!(" -> {}", self.type_text(r)))
                    .unwrap_or_default();
                format!("fn({}){returns}", params.join(", "))
            }
        }
    }

    /// A pattern as it would be written.
    pub fn pattern_text(&self, id: Id<Pattern>) -> String {
        match &self.patterns[id] {
            Pattern::Wildcard => "_".into(),
            Pattern::Binding { name, mutable } => {
                format!("{}{name}", if *mutable { "mut " } else { "" })
            }
            Pattern::Int(n) => n.to_string(),
            Pattern::Bool(b) => b.to_string(),
            Pattern::Str(s) => format!("{s:?}"),
            Pattern::None => "none".into(),
            Pattern::Failed(inner) => format!("fail {}", self.pattern_text(*inner)),
            Pattern::Variant { path, fields } => match fields {
                PatternFields::Unit => path.to_string(),
                PatternFields::Tuple(fields) => {
                    let fields: Vec<String> =
                        fields.iter().map(|&f| self.pattern_text(f)).collect();
                    format!("{path}({})", fields.join(", "))
                }
                PatternFields::Named { fields, rest } => {
                    let mut parts: Vec<String> = (fields.iter())
                        .map(|(name, p)| format!("{name}: {}", self.pattern_text(*p)))
                        .collect();
                    if *rest {
                        parts.push("..".into());
                    }
                    format!("{path} {{ {} }}", parts.join(", "))
                }
            },
        }
    }
}
