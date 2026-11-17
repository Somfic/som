use som_base::message::code;
use som_base::{Diagnostic, Fix, Label, msg};
use som_borrowcheck::BorrowError;
use som_resolve::ResolveError;
use som_typeck::TypeError;

use crate::{LibraryError, LibraryWarning, LoadError};

/// An error from any pass, keeping its pass-specific type.
#[derive(Debug)]
pub enum Error {
    Load(LoadError),
    Library(LibraryError),
    Resolve(ResolveError),
    Type(TypeError),
    Prove(som_prove::ProveError),
    Borrow(BorrowError),
    Comptime(som_comptime::ComptimeError),
    /// An error in one instance of a function checked for each instance (`@if N >= 3`):
    /// `describe<2>`, first used at `used_at`.
    InInstance {
        instance: String,
        used_at: som_base::Span,
        inner: Box<Error>,
    },
}

impl Error {
    /// The name of the specific error, like `Mismatch` or `UseAfterMove`.
    pub fn kind(&self) -> String {
        let debug = match self {
            Error::Load(e) => format!("{e:?}"),
            Error::Library(e) => format!("{e:?}"),
            Error::Resolve(e) => format!("{e:?}"),
            Error::Type(e) => format!("{e:?}"),
            Error::Prove(e) => format!("{e:?}"),
            Error::Borrow(e) => format!("{e:?}"),
            Error::Comptime(e) => format!("{e:?}"),
            Error::InInstance { inner, .. } => return inner.kind(),
        };
        variant_name(&debug)
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            Error::Load(e) => e.to_diagnostic(),
            Error::Library(e) => e.to_diagnostic(),
            Error::Resolve(e) => e.to_diagnostic(),
            Error::Type(e) => e.to_diagnostic(),
            Error::Prove(e) => e.to_diagnostic(),
            Error::Borrow(e) => e.to_diagnostic(),
            Error::Comptime(e) => e.to_diagnostic(),
            Error::InInstance {
                instance,
                used_at,
                inner,
            } => {
                let mut diagnostic = inner.to_diagnostic();
                diagnostic
                    .message
                    .0
                    .insert(0, format!("in `{instance}`: ").into());
                diagnostic.with_label(Label::secondary(
                    used_at.clone(),
                    msg![code(instance.clone()), " is used here"],
                ))
            }
        }
    }
}

/// Something worth fixing that doesn't stop a program compiling.
#[derive(Debug)]
pub enum Warning {
    Library(LibraryWarning),
    /// `fib(20)`, which could be worked out while compiling: `@fib(20)`. A hint, not a
    /// warning: nothing's wrong with it.
    CouldBeConst(som_comptime::CouldBeConst),
}

impl Warning {
    /// The name of the specific warning, like `DeclaredInHeader`.
    pub fn kind(&self) -> String {
        let debug = match self {
            Warning::Library(w) => format!("{w:?}"),
            Warning::CouldBeConst(w) => format!("{w:?}"),
        };
        variant_name(&debug)
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            Warning::Library(w) => w.to_diagnostic(),
            Warning::CouldBeConst(w) => could_be_const_diagnostic(w),
        }
    }
}

/// `UseAfterMove` of `UseAfterMove { … }`.
fn variant_name(debug: &str) -> String {
    debug
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Said like a moved value is: where it is, where what it's given comes from, why, then
/// what to do about it.
fn could_be_const_diagnostic(w: &som_comptime::CouldBeConst) -> Diagnostic {
    let result = match &w.value {
        Some(value) => msg![code(value.clone())],
        None => msg!["the same value"],
    };
    let mut d = Diagnostic::hint(msg![
        code(w.call.clone()),
        " could be computed during compilation"
    ])
    .with_label(Label::primary(
        w.span.clone(),
        msg!["will always be ", result.clone(),],
    ));
    for (name, value, set_at) in &w.known {
        d = d.with_label(Label::secondary(
            set_at.clone(),
            msg![code(name.clone()), " will always be ", code(value.clone())],
        ));
    }
    d.with_hint(msg![
        code(w.call.clone()),
        " is a pure function, its arguments will never change, so its value won't either: ",
        result.clone()
    ])
    .with_hint(msg![
        code(w.fix.clone()),
        " would compute it during compilation, and use ",
        result,
        " instead"
    ])
    .with_hint("if this should run each time (it's quick, or it's being timed), leave it as it is")
    .with_fix(Fix::replace(
        msg!["work it out while compiling: ", code(w.fix.clone())],
        w.span.clone(),
        w.fix.clone(),
    ))
}
