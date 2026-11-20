use som_base::message::code;
use som_base::{Diagnostic, Label, Message, Span, msg};

/// Why a shader can't be compiled for the GPU, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderError {
    /// The shader being compiled.
    pub shader: String,
    pub message: Message,
    pub span: Span,
    pub label: Message,
    pub hint: Option<Message>,
    /// How the shader gets to `span`: each call from the shader in, by the function it calls.
    pub calls: Vec<(String, Span)>,
}

impl ShaderError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        let mut d = Diagnostic::error(self.message.clone())
            .with_label(Label::primary(self.span.clone(), self.label.clone()));
        for (name, span) in &self.calls {
            d = d.with_label(Label::secondary(
                span.clone(),
                msg![code(format!("{name}(…)")), " is called here"],
            ));
        }
        d = d.with_hint(msg![
            "while compiling shader ",
            code(&self.shader),
            " for the GPU"
        ]);
        match &self.hint {
            Some(hint) => d.with_hint(hint.clone()),
            None => d,
        }
    }
}

/// What went wrong, before it's known how the shader got there.
pub(crate) struct Problem {
    pub message: Message,
    pub span: Span,
    pub label: Message,
    pub hint: Option<Message>,
}

/// A type the GPU doesn't have: `what` it is, and what to use instead.
#[derive(Debug)]
pub(crate) struct NoGpu {
    pub what: Message,
    pub hint: Option<Message>,
}

impl NoGpu {
    pub fn new(what: impl Into<Message>, hint: Option<Message>) -> Self {
        NoGpu {
            what: what.into(),
            hint,
        }
    }

    /// The problem it is at `span`, where `label` says what has the type.
    pub fn at(self, span: &Span, label: Message) -> Problem {
        Problem {
            message: msg!["the GPU has no ", self.what],
            span: span.clone(),
            label,
            hint: self.hint,
        }
    }
}
