use codespan_reporting::term::{
    termcolor::{ColorChoice, StandardStream},
    Config,
};

use crate::{files::Files, scanner::lexeme::Lexeme};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diagnostic<'a> {
    pub severity: Severity,
    pub title: String,
    pub errors: Vec<Error<'a>>,
}

impl<'a> Diagnostic<'a> {
    pub fn print(&self, files: &'a Files) {
        codespan_reporting::term::emit(
            &mut StandardStream::stderr(ColorChoice::Auto),
            &Config::default(),
            files,
            &self.clone().into(),
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Error<'a> {
    pub message: String,
    pub label: Label,
    pub range: Range<'a>,
}

impl<'a> Error<'a> {
    pub fn primary(
        file_id: impl Into<&'a str>,
        position: usize,
        length: usize,
        message: impl Into<String>,
    ) -> Error<'a> {
        Error::new(file_id, Label::Primary, position, length, message)
    }

    pub fn secondary(
        file_id: impl Into<&'a str>,
        position: usize,
        length: usize,
        message: impl Into<String>,
    ) -> Error<'a> {
        Error::new(file_id, Label::Secondary, position, length, message)
    }

    pub fn new(
        file_id: impl Into<&'a str>,
        label: Label,
        position: usize,
        length: usize,
        message: impl Into<String>,
    ) -> Error<'a> {
        Error {
            message: message.into(),
            label,
            range: Range {
                file_id: file_id.into(),
                position,
                length,
            },
        }
    }

    pub(crate) fn transform_range(mut self, lexemes: &'a [Lexeme<'a>]) -> Error {
        self.range = self.range.to_source_code_range(lexemes);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Label {
    Primary,
    Secondary,
}

impl From<Severity> for codespan_reporting::diagnostic::Severity {
    fn from(val: Severity) -> Self {
        match val {
            Severity::Error => codespan_reporting::diagnostic::Severity::Error,
            Severity::Warning => codespan_reporting::diagnostic::Severity::Warning,
            Severity::Note => codespan_reporting::diagnostic::Severity::Note,
            Severity::Help => codespan_reporting::diagnostic::Severity::Help,
        }
    }
}

impl<'a> Diagnostic<'a> {
    pub fn error(message: impl Into<String>) -> Diagnostic<'a> {
        Diagnostic::new(Severity::Error, message)
    }

    pub fn warning(message: impl Into<String>) -> Diagnostic<'a> {
        Diagnostic::new(Severity::Warning, message)
    }

    pub fn new(severity: Severity, message: impl Into<String>) -> Diagnostic<'a> {
        Diagnostic {
            severity,
            title: message.into(),
            errors: vec![],
        }
    }

    pub fn with_error(mut self, error: Error<'a>) -> Self {
        self.errors.push(error);
        self
    }
}

impl<'a> From<Diagnostic<'a>> for codespan_reporting::diagnostic::Diagnostic<&'a str> {
    fn from(val: Diagnostic<'a>) -> codespan_reporting::diagnostic::Diagnostic<&'a str> {
        codespan_reporting::diagnostic::Diagnostic::<&'a str>::new(val.severity.into())
            .with_message(val.title)
            .with_labels(val.errors.into_iter().map(|error| error.into()).collect())
    }
}

impl<'a> From<Error<'a>> for codespan_reporting::diagnostic::Label<&'a str> {
    fn from(val: Error<'a>) -> codespan_reporting::diagnostic::Label<&'a str> {
        codespan_reporting::diagnostic::Label::new(
            val.label.into(),
            val.range.file_id,
            val.range.position..val.range.position + val.range.length,
        )
        .with_message(val.message)
    }
}

impl From<Label> for codespan_reporting::diagnostic::LabelStyle {
    fn from(val: Label) -> Self {
        match val {
            Label::Primary => codespan_reporting::diagnostic::LabelStyle::Primary,
            Label::Secondary => codespan_reporting::diagnostic::LabelStyle::Secondary,
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Range<'a> {
    pub file_id: &'a str,
    pub position: usize,
    pub length: usize,
}

impl<'a> Range<'a> {
    pub fn to_source_code_range(self, lexemes: &[Lexeme]) -> Self {
        let start = if self.position >= lexemes.len() {
            let last_lexeme = &lexemes[lexemes.len() - 1].range;
            last_lexeme.position + 1
        } else {
            let start_lexeme = &lexemes[self.position].range;
            start_lexeme.position
        };

        let end = if self.position + self.length >= lexemes.len() {
            let last_lexeme = &lexemes[lexemes.len() - 1].range;
            last_lexeme.position + last_lexeme.length
        } else {
            let end_lexeme = &lexemes[self.position + self.length].range;
            end_lexeme.position + end_lexeme.length
        };

        Range {
            file_id: self.file_id,
            position: start,
            length: end - start,
        }
    }
}
