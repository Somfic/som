use crate::limits::STEP_LIMIT;
use som_base::message::code;
use som_base::{Diagnostic, Label, Span, msg};

/// One step of the code running while compiling, from the innermost out: a call, at `span`,
/// working out `name`; last, the `@const` being worked out, at its declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Working {
    pub name: String,
    pub span: Span,
    /// A call of function `name`, rather than the constant `name`.
    pub call: bool,
}

/// Why a `@const`'s value couldn't be worked out. `span` is where running stopped; `trace`
/// is how it got there.
#[derive(Debug, PartialEq, Eq)]
pub enum ComptimeError {
    ConstCallsExtern {
        function: String,
        /// Why it can't be: it only exists when the program runs, or its library can't be
        /// loaded.
        reason: String,
        span: Span,
        trace: Vec<Working>,
    },
    ConstReachedUnreachable {
        span: Span,
        trace: Vec<Working>,
    },
    /// It divided by zero, or, with `overflow`, worked out `MIN / -1`.
    ConstDividedByZero {
        overflow: bool,
        span: Span,
        trace: Vec<Working>,
    },
    /// It ran for longer than `STEP_LIMIT` steps: most likely, it never ends.
    ConstTooLong {
        span: Span,
        trace: Vec<Working>,
    },
    ConstCycle {
        name: String,
        span: Span,
        trace: Vec<Working>,
    },
    /// It read or wrote memory it doesn't have: freed, past its end, or through a null
    /// pointer. Only unchecked pointer code can.
    ConstOutOfBounds {
        span: Span,
        trace: Vec<Working>,
    },
    /// It reached `@fail(message)`: a check that didn't hold, like `assert`'s.
    ConstFailed {
        message: String,
        span: Span,
        trace: Vec<Working>,
    },
    ConstNotSupported {
        what: &'static str,
        span: Span,
        trace: Vec<Working>,
    },
    /// Its value can't be put in the program: it holds memory, or is a kind of value that
    /// can't be kept yet.
    ConstHoldsMemory {
        name: String,
        /// The part of it that can't be: `NUMBERS.mem.ptr`. `None` for all of it.
        part: Option<String>,
        what: &'static str,
        span: Span,
    },
}

impl ComptimeError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        let (d, span, label, trace) = match self {
            ComptimeError::ConstCallsExtern {
                function,
                reason,
                span,
                trace,
            } => (
                Diagnostic::error(msg![
                    "a constant can't call ",
                    code(function),
                    " while compiling"
                ])
                .with_hint(reason.clone()),
                span,
                msg!["calls ", code(function)],
                trace,
            ),
            ComptimeError::ConstReachedUnreachable { span, trace } => (
                Diagnostic::error(msg!["this constant reached ", code("@unreachable")]),
                span,
                msg!["reached here"],
                trace,
            ),
            ComptimeError::ConstDividedByZero {
                overflow: false,
                span,
                trace,
            } => (
                Diagnostic::error("this constant divides by zero"),
                span,
                msg!["divides by zero here"],
                trace,
            ),
            ComptimeError::ConstDividedByZero {
                overflow: true,
                span,
                trace,
            } => (
                Diagnostic::error("this constant divides the smallest number by -1")
                    .with_hint("its result is one too big for its type"),
                span,
                msg!["divides here"],
                trace,
            ),
            ComptimeError::ConstTooLong { span, trace } => (
                Diagnostic::error("this constant takes too long to work out").with_hint(format!(
                    "it ran {STEP_LIMIT} steps without finishing: it may never end"
                )),
                span,
                msg!["still running here"],
                trace,
            ),
            ComptimeError::ConstCycle { name, span, trace } => (
                Diagnostic::error(msg!["constant ", code(name), " is made of itself"]),
                span,
                msg!["needs ", code(name), " to work out ", code(name)],
                trace,
            ),
            ComptimeError::ConstOutOfBounds { span, trace } => (
                Diagnostic::error("this constant uses memory it doesn't have")
                    .with_hint("memory that's freed, past the end of its allocation, or null"),
                span,
                msg!["used here"],
                trace,
            ),
            ComptimeError::ConstFailed {
                message,
                span,
                trace,
            } => (
                Diagnostic::error(message.as_str()),
                span,
                msg!["failed here, while compiling"],
                trace,
            ),
            ComptimeError::ConstNotSupported { what, span, trace } => (
                Diagnostic::error(msg!["a constant can't use ", *what, " yet"]),
                span,
                msg!["used here, while compiling"],
                trace,
            ),
            ComptimeError::ConstHoldsMemory {
                name,
                part,
                what,
                span,
            } => {
                let label = match part {
                    Some(part) => msg![code(part), " is ", *what],
                    None => msg!["its value is ", *what],
                };
                return Diagnostic::error(msg![
                    "constant ",
                    code(name),
                    " can't be put in the program"
                ])
                .with_label(Label::primary(span.clone(), label))
                .with_hint(
                    "a constant's value can be numbers, bools, text, and structs, enums, \
                     arrays and optionals of those",
                );
            }
        };
        let mut d = d.with_label(Label::primary(span.clone(), label));
        // Each place once: recursion calls from the same few places, many times over.
        let mut shown: Vec<&Working> = Vec::new();
        for working in trace {
            if !shown
                .iter()
                .any(|w| w.span == working.span && w.call == working.call)
            {
                shown.push(working);
            }
        }
        for working in &shown {
            let label = match working.call {
                true => msg![code(format!("{}(…)", working.name)), " is called here"],
                false => msg!["while working out ", code(&working.name)],
            };
            d = d.with_label(Label::secondary(working.span.clone(), label));
        }
        let calls = trace.iter().filter(|w| w.call).count();
        let shown_calls = shown.iter().filter(|w| w.call).count();
        if calls > shown_calls {
            d = d.with_hint(format!("it was {calls} calls deep"));
        }
        d
    }
}
