use som_base::message::{Message, Part, code, func, struct_name, ty, var};
use som_base::{Diagnostic, Fix, Label, Span, msg};

#[derive(Debug, Clone)]
pub enum ResolveError {
    /// `impl [i32; 3] { … }`: methods for a type that's neither a struct nor a built-in one.
    MethodsOnType { ty: String, span: Span },
    /// A function, struct, alias or trait used where it can't be seen: private to another
    /// file or type, or only visible in another module.
    NotVisible {
        name: String,
        span: Span,
        hidden: crate::visibility::Hidden,
    },
    UnboundVariable {
        name: String,
        span: Span,
        help: Option<Message>,
        /// Changes that would fix it: the suggested name, or a `use`.
        fixes: Vec<Fix>,
    },
    UnknownFunction {
        name: String,
        span: Span,
        help: Option<Message>,
        fixes: Vec<Fix>,
    },
    UnknownStruct {
        name: String,
        span: Span,
        help: Option<Message>,
        fixes: Vec<Fix>,
    },
    UnknownType {
        name: String,
        span: Span,
        help: Option<Message>,
        fixes: Vec<Fix>,
    },
    UnknownTrait {
        name: String,
        span: Span,
        help: Option<Message>,
        fixes: Vec<Fix>,
    },
    DuplicateDefinition {
        name: String,
        span: Span,
        previous: Span,
    },
    /// `prop x: i32` on a struct with a field `x` as visible: `p.x` couldn't say which.
    PropNamedLikeField {
        name: String,
        span: Span,
        field: Span,
    },
    /// `prop len<T> -> …`: a property can't have type parameters of its own.
    PropGenerics { span: Span },
    /// `c::puts(…)` in a module without `extern "c"`.
    LibraryNotDeclared {
        library: String,
        span: Span,
        fix: Fix,
    },
    /// `@const DOUBLE = n * 2;` in a function: a constant's value can't name its locals.
    ConstNotKnown { span: Span },
    /// `@on start`: an event there isn't. `fail` is the one there is.
    UnknownEvent { name: String, span: Span },
    /// `@on fail` on a method: a listener is a function of its own.
    ListenerIsMethod { span: Span },
    /// `@test` on a method: a test is a function of its own.
    TestIsMethod { span: Span },
    /// `parses_hex()` where `parses_hex` is a `@test`: only the test runner calls one.
    TestCalled { name: String, span: Span, defined_at: Span },
    /// `@const A = B; const B = A;`: a constant made of itself.
    ConstCycle { name: String, span: Span },
    /// `break x;` or `continue x;` where `x` isn't a `for` loop's variable.
    NotALoop {
        keyword: &'static str,
        name: String,
        span: Span,
        defined_at: Span,
    },
}

impl ResolveError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        type Parts<'a> = (&'a str, Part, &'a Span, &'a Option<Message>, &'a Vec<Fix>);
        let (what, name, span, help, fixes): Parts = match self {
            ResolveError::UnboundVariable {
                name,
                span,
                help,
                fixes,
            } => ("variable", var(name), span, help, fixes),
            ResolveError::UnknownFunction {
                name,
                span,
                help,
                fixes,
            } => ("function", func(name), span, help, fixes),
            ResolveError::UnknownStruct {
                name,
                span,
                help,
                fixes,
            } => ("struct", struct_name(name), span, help, fixes),
            ResolveError::UnknownType {
                name,
                span,
                help,
                fixes,
            } => ("type", ty(name), span, help, fixes),
            ResolveError::UnknownTrait {
                name,
                span,
                help,
                fixes,
            } => ("trait", ty(name), span, help, fixes),
            ResolveError::NotVisible { name, span, hidden } => {
                return crate::visibility::diagnostic(name, span, hidden);
            }
            ResolveError::MethodsOnType { ty, span } => {
                return Diagnostic::error(msg!["cannot add methods to ", code(ty.clone())])
                    .with_label(Label::primary(
                        span.clone(),
                        "only structs and built-in types like `i32` have methods of their own",
                    ))
                    .with_hint("a trait can still be implemented for it");
            }
            ResolveError::DuplicateDefinition {
                name,
                span,
                previous,
            } => {
                return Diagnostic::error(msg![var(name), " is defined more than once"])
                    .with_label(Label::primary(span.clone(), "redefined here"))
                    .with_label(Label::secondary(previous.clone(), "first defined here"));
            }
            ResolveError::PropNamedLikeField { name, span, field } => {
                return Diagnostic::error(msg![
                    "property ",
                    var(name),
                    " has the same name as a field"
                ])
                .with_label(Label::primary(span.clone(), "property defined here"))
                .with_label(Label::secondary(field.clone(), "field defined here"))
                .with_hint(msg![
                    "both are read as ",
                    code(format!("x.{name}")),
                    "; rename one, or make the property more visible than the field, so code that can't see the field reads the property"
                ]);
            }
            ResolveError::PropGenerics { span } => {
                return Diagnostic::error("a property can't have type parameters")
                    .with_label(Label::primary(span.clone(), "type parameters here"))
                    .with_hint(msg![
                        "for one with type parameters, use a method: ",
                        code("fn")
                    ]);
            }
            ResolveError::LibraryNotDeclared { library, span, fix } => {
                return Diagnostic::error(msg![code(library.clone()), " isn't declared here"])
                    .with_label(Label::primary(
                        span.clone(),
                        msg![
                            "a library is used where its ",
                            code(format!("extern \"{library}\"")),
                            " is"
                        ],
                    ))
                    .with_fix(fix.clone());
            }
            ResolveError::ConstNotKnown { span } => {
                return Diagnostic::error("a constant's value has to be known while compiling")
                    .with_label(Label::primary(span.clone(), "only known when the program runs"))
                    .with_hint("it's worked out while compiling, before the function runs: its parameters and variables don't have values yet");
            }
            ResolveError::UnknownEvent { name, span } => {
                return Diagnostic::error(msg!["there's no event ", code(name)])
                    .with_label(Label::primary(span.clone(), "not an event"))
                    .with_hint(msg![code("@on fail"), ": called when the program fails"]);
            }
            ResolveError::ListenerIsMethod { span } => {
                return Diagnostic::error(msg![code("@on fail"), " is for a function of its own"])
                    .with_label(Label::primary(span.clone(), "a method"))
                    .with_hint("move it out of the impl block");
            }
            ResolveError::TestIsMethod { span } => {
                return Diagnostic::error(msg![code("@test"), " is for a function of its own"])
                    .with_label(Label::primary(span.clone(), "a method"))
                    .with_hint("move it out of the impl block");
            }
            ResolveError::TestCalled {
                name,
                span,
                defined_at,
            } => {
                return Diagnostic::error(msg![func(name), " is a test, run by ", code("som test")])
                    .with_label(Label::primary(span.clone(), "only the test runner calls it"))
                    .with_label(Label::secondary(defined_at.clone(), msg![code("@test"), " here"]))
                    .with_hint(msg![
                        "move what's shared into a ",
                        code("fn"),
                        " without ",
                        code("@test"),
                        ", and call that"
                    ]);
            }
            ResolveError::ConstCycle { name, span } => {
                return Diagnostic::error(msg!["constant ", var(name), " is made of itself"])
                    .with_label(Label::primary(span.clone(), "its value comes back to it"));
            }
            ResolveError::NotALoop {
                keyword,
                name,
                span,
                defined_at,
            } => {
                return Diagnostic::error(msg![var(name), " isn't a loop"])
                    .with_label(Label::primary(
                        span.clone(),
                        msg![
                            code(*keyword),
                            " needs the variable of a ",
                            code("for"),
                            " around it"
                        ],
                    ))
                    .with_label(Label::secondary(defined_at.clone(), "defined here"));
            }
        };

        let mut diagnostic = Diagnostic::error(msg!["cannot find ", what, " ", name])
            .with_label(Label::primary(span.clone(), "not found in this scope"));
        if let Some(help) = help {
            diagnostic = diagnostic.with_hint(help.clone());
        }
        for fix in fixes {
            diagnostic = diagnostic.with_fix(fix.clone());
        }
        diagnostic
    }
}
