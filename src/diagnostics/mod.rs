use std::{error::Error as ThisError, fmt::Display};

use cranelift::{module::ModuleError, object::object};
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
    #[error(transparent)]
    LinkerError(LinkerError),
    #[error(transparent)]
    RunnerError(RunnerError),
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
    #[error("expected a condition")]
    ExpectedCondition,
    #[error("expected an else branch")]
    ExpectedElseBranch,
    #[error("expected a function")]
    ExpectedFunction,
    #[error("expected a parameter list")]
    ExpectedParameterList,
    #[error("expected ')' to end parameter list")]
    ExpectedParameterListEnd,
    #[error("expected a type annotation")]
    ExpectedTypeAnnotation,
    #[error("expected an argument list")]
    ExpectedArgumentList,
    #[error("expected ')' to end argument list")]
    ExpectedArgumentListEnd,
    #[error("expected a type")]
    ExpectedType,
    #[error("expected a type definition")]
    ExpectedTypeDefinition,
    #[error("expected a field")]
    ExpectedField,
    #[error("expected a struct")]
    ExpectedStruct,
    #[error("expected a '.' for field access")]
    ExpectedFieldAccess,
    #[error("expected an extern definition")]
    ExpectedExternDefinition,
    #[error("expected an extern function alias")]
    ExpectedExternFunctionAlias,
    #[error("expected an extern function definition")]
    ExpectedExternFunctionDefinition,
    #[error("expected a function type")]
    ExpectedFunctionType,
    #[error("expected a parameter")]
    ExpectedParameter,
    #[error("expected a while loop")]
    ExpectedWhile,
    #[error("expected an assignment")]
    ExpectedAssignment,
    #[error("invalid visibility modifier")]
    InvalidVisibilityModifier,
    #[error("expected an import statement")]
    ExpectedImport,
    #[error("expected a value definition")]
    ExpectedValueDefinition,
    #[error("expected a path segment")]
    ExpectedSegment,
}

#[derive(Debug, thiserror::Error)]
pub enum TypeCheckError {
    #[error("undefined variable")]
    UndefinedVariable,
    #[error("type mismatch")]
    TypeMismatch,
    #[error("unexpected type")]
    ExpectedType,
    #[error("not a function")]
    NotAFunction,
    #[error("argument mismatch")]
    ArgumentCountMismatch,
    #[error("undefined type")]
    UndefinedType,
    #[error("expected a struct")]
    ExpectedStruct,
    #[error("expected a field")]
    ExpectedField,
    #[error("this type has recursion")]
    RecursiveType,
}

#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error("undefined variable")]
    UndefinedVariable,
    #[error(transparent)]
    ModuleError(#[from] ModuleError),
    #[error(transparent)]
    WriteError(#[from] object::write::Error),
    #[error("undefined function")]
    UndefinedFunction,
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("invalid assignment target")]
    InvalidAssignmentTarget,
}

#[derive(Debug, thiserror::Error)]
pub enum LinkerError {
    #[error("could not find a c linker on the system")]
    NoLinkerFound,
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("failed to link executable")]
    FailedToLink,
}

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("failed to execute the program")]
    ExecutionFailed,
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

impl LinkerError {
    pub fn to_diagnostic(self) -> Diagnostic {
        Diagnostic::from(Error::LinkerError(self))
    }
}

impl RunnerError {
    pub fn to_diagnostic(self) -> Diagnostic {
        Diagnostic::from(Error::RunnerError(self))
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
            writeln!(f, "| --> {}", label.span.source.identifier())?;
            writeln!(
                f,
                "| {}",
                label
                    .span
                    .source
                    .content()
                    .lines()
                    .nth(label.span.start.line - 1)
                    .unwrap_or("")
            )?;

            write!(
                f,
                "| {}",
                " ".repeat(label.span.start.col.saturating_sub(1))
            )?;

            writeln!(
                f,
                "{} {}",
                "~".repeat(label.span.length.max(1)).bright_black(),
                label.message.bright_red()
            )?;
        }
        for hint in &self.hints {
            writeln!(f, "= hint: {}", hint)?;
        }
        for trace in &self.trace {
            writeln!(f, "= caused by: {}", trace)?;
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
    pub fn label(&self, message: impl Into<String>) -> Label {
        Label {
            message: message.into(),
            span: self.clone(),
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
