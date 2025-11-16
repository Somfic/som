use std::{error::Error as ThisError, fmt::Display};

use cranelift::module::ModuleError;
use owo_colors::OwoColorize;

use crate::{lexer::Cursor, Span};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    LexicalError(LexicalError),
    #[error(transparent)]
    ParserError(ParserError),
    #[error(transparent)]
    TypeCheckError(TypeCheckError),
    #[error(transparent)]
    EmitError(EmitError),
}

#[derive(Debug, thiserror::Error)]
pub enum LexicalError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ParserError {
    #[error("expected an expression")]
    ExpectedExpression,
    #[error("expected a statement")]
    ExpectedStatement,
    #[error("invalid primary expression")]
    InvalidPrimaryExpression,
    #[error("invalid unary expression")]
    InvalidUnaryExpression,
    #[error("invalid binary operator")]
    InvalidBinaryOperator,
    #[error("unexpected end of input")]
    UnexpectedEndOfInput,
    #[error("expected '(' to start a group")]
    ExpectedGroupStart,
    #[error("expected ')' to end a group")]
    ExpectedGroupEnd,
    #[error("expected '{{' to start a scope")]
    ExpectedScopeStart,
    #[error("expected '}}' to end a scope")]
    ExpectedScopeEnd,
    #[error("expected a semicolon")]
    ExpectedSemicolon,
    #[error("expected '{{' to start a block")]
    ExpectedBlockStart,
    #[error("expected '}}' to end a block")]
    ExpectedBlockEnd,
    #[error("invalid returning expression")]
    InvalidReturningExpression,
    #[error("expected a declaration")]
    ExpectedDeclaration,
    #[error("expected an identifier")]
    ExpectedIdentifier,
    #[error("expected a value")]
    ExpectedValue,
}

#[derive(Debug, thiserror::Error)]
pub enum TypeCheckError {
    #[error("undefined variable '{0}'")]
    UndefinedVariable(String),
}

#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error("undefined variable '{0}'")]
    UndefinedVariable(String),
    #[error(transparent)]
    ModuleError(#[from] ModuleError),
}

impl Error {
    pub fn to_diagnostic(self) -> Diagnostic {
        Diagnostic::from(self)
    }
}

impl ParserError {
    pub fn to_diagnostic(self) -> Diagnostic {
        Diagnostic::from(Error::ParserError(self))
    }
}

impl LexicalError {
    pub fn to_diagnostic(self) -> Diagnostic {
        Diagnostic::from(Error::LexicalError(self))
    }
}

impl TypeCheckError {
    pub fn to_diagnostic(self) -> Diagnostic {
        Diagnostic::from(Error::TypeCheckError(self))
    }
}

impl EmitError {
    pub fn to_diagnostic(self) -> Diagnostic {
        Diagnostic::from(Error::EmitError(self))
    }
}

impl From<Error> for Diagnostic {
    fn from(error: Error) -> Self {
        let mut trace = vec![];
        let mut current_error: &dyn ThisError = &error;
        while let Some(parent) = current_error.source() {
            trace.push(parent.to_string());
            current_error = parent;
        }

        Diagnostic {
            severity: Severity::Error,
            trace,
            message: error.to_string(),
            hints: vec![],
            labels: vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub trace: Vec<String>, // todo: find better type for this
    pub message: String,
    pub hints: Vec<String>,
    pub labels: Vec<Label>,
}

impl Diagnostic {
    pub fn with_label(mut self, label: impl Into<Label>) -> Self {
        self.labels.push(label.into());
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hints.push(hint.into());
        self
    }

    pub fn with_cause<E: ThisError + 'static>(mut self, cause: E) -> Self {
        self.trace.push(cause.to_string());
        self
    }

    pub fn to_err<T>(self) -> Result<T, Self> {
        Err(self)
    }
}

impl Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}: {}", self.severity, self.message)?;
        for label in &self.labels {
            writeln!(f)?;
            writeln!(
                f,
                "{}",
                label
                    .span
                    .source
                    .content()
                    .lines()
                    .nth(label.span.start.line - 1)
                    .unwrap_or("")
            )?;

            write!(f, "{}", " ".repeat(label.span.start.col.saturating_sub(1)))?;

            writeln!(
                f,
                "{} {}",
                "~".repeat(label.span.length.max(1)).bright_black(),
                label.message.bright_red()
            )?;
        }
        for hint in &self.hints {
            writeln!(f, "hint: {}", hint)?;
        }
        for trace in &self.trace {
            writeln!(f, "caused by: {}", trace)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "{}", "error".red().bold()),
            Severity::Warning => write!(f, "{}", "warning".yellow().bold()),
            Severity::Note => write!(f, "{}", "note".blue().bold()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    pub message: String,
    pub span: Span,
}

impl Span {
    pub fn label(self, message: impl Into<String>) -> Label {
        Label {
            message: message.into(),
            span: self,
        }
    }
}

impl Cursor {
    pub fn label(&self, message: impl Into<String>) -> Label {
        let span = Span {
            start: self.position,
            end: self.position,
            start_offset: self.byte_offset,
            length: 0,
            source: self.source.clone(),
        };

        Label {
            message: message.into(),
            span,
        }
    }
}
