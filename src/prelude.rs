pub use crate::compiler::Compiler;
pub use crate::expressions::binary::BinaryExpression;
pub use crate::expressions::binary::BinaryOperator;
pub use crate::expressions::conditional::ConditionalExpression;
pub use crate::expressions::function::Parameter;
pub use crate::expressions::primary::PrimaryExpression;
pub use crate::expressions::struct_constructor::StructConstructorExpression;
pub use crate::expressions::ExpressionValue;
pub use crate::expressions::TypedExpressionValue;
pub use crate::lexer::Identifier;
pub use crate::parser::lookup::{BindingPower, Lookup};
pub use crate::runner::Runner;
pub use crate::statements::extern_declaration::ExternDeclarationStatement;
pub use crate::statements::type_declaration::TypeDeclarationStatement;
pub use crate::statements::variable_declaration::VariableDeclarationStatement;
pub use crate::statements::GenericStatement;
pub use crate::statements::{Statement, StatementValue};
pub use crate::type_checker::TypeChecker;
pub use crate::types::FunctionType;
pub use crate::types::StructType;
pub use crate::types::{Type, TypeValue};
pub use crate::{
    expressions::{Expression, TypedExpression},
    lexer::{Lexer, Token, TokenKind, TokenValue},
};
pub use crate::{parser::Parser, statements::TypedStatement};
pub use cranelift::prelude::types as CompilerType;
pub use cranelift::prelude::{FunctionBuilder, InstBuilder};
pub use miette::Diagnostic;
use miette::LabeledSpan;
use miette::SourceSpan;
use nucleo_matcher::pattern::AtomKind;
use nucleo_matcher::pattern::CaseMatching;
use nucleo_matcher::pattern::Normalization;
use nucleo_matcher::pattern::Pattern;
use nucleo_matcher::Config;
use nucleo_matcher::Matcher;
use std::fmt::Display;
use std::ops::Sub;
use thiserror::Error;

pub type CompileValue = cranelift::prelude::Value;
pub type Result<T> = std::result::Result<T, Error>;
pub type Results<T> = std::result::Result<T, Vec<Error>>;

pub type CompileEnvironment<'env> = crate::compiler::Environment<'env>;
pub type TypeEnvironment<'env> = crate::type_checker::Environment<'env>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span(pub miette::SourceSpan);

impl Default for Span {
    fn default() -> Self {
        Span(miette::SourceSpan::new(0.into(), 0))
    }
}

impl Span {
    pub fn label(&self, message: impl Into<String>) -> LabeledSpan {
        LabeledSpan::new(Some(message.into()), self.0.offset(), self.0.len())
    }
}

impl std::ops::Add for Span {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let spans = [self.0, rhs.0];

        let start = spans.iter().map(|s| s.offset()).min().unwrap_or(0);

        let end = spans
            .iter()
            .map(|s: &SourceSpan| s.offset() + s.len())
            .max()
            .unwrap_or(start);

        let length = end.sub(start);

        Span(miette::SourceSpan::new(start.into(), length))
    }
}

impl Span {
    pub fn new(start: usize, length: usize) -> Self {
        Span(miette::SourceSpan::new(start.into(), length))
    }

    pub fn offset(&self) -> usize {
        self.0.offset()
    }

    pub fn length(&self) -> usize {
        self.0.len()
    }
}

impl From<Span> for SourceSpan {
    fn from(span: Span) -> Self {
        span.0
    }
}

impl From<SourceSpan> for Span {
    fn from(span: SourceSpan) -> Self {
        Span(span)
    }
}

#[derive(Clone, Error, Debug, Diagnostic)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Lexer(#[from] LexerError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Parser(#[from] ParserError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    TypeChecker(#[from] TypeCheckerError),
}

#[derive(Clone, Error, Debug, Diagnostic)]
pub enum LexerError {
    #[error("unexpected character")]
    #[diagnostic()]
    UnexpectedCharacter {
        #[label("this character was not expected")]
        range: (usize, usize),

        #[help]
        help: String,
    },

    #[error("improper number")]
    #[diagnostic()]
    ImproperNumber {
        #[label("this is not a valid number")]
        range: (usize, usize),

        #[help]
        help: String,
    },

    #[error("improper character")]
    #[diagnostic()]
    ImproperCharacter {
        #[label("this is not a valid character")]
        range: (usize, usize),

        #[help]
        help: String,
    },
}

#[derive(Clone, Error, Debug, Diagnostic)]
pub enum ParserError {
    #[error("unexpected token")]
    #[diagnostic()]
    UnexpectedToken {
        #[label("this token was not expected")]
        token: Token,

        #[help]
        help: String,
    },

    #[error("unexpected end of file")]
    #[diagnostic()]
    UnexpectedEndOfFile {
        #[label(collection)]
        labels: Vec<LabeledSpan>,
        #[help]
        help: String,
    },

    #[error("expected expression")]
    #[diagnostic()]
    ExpectedExpression {
        #[label("expected an expression here")]
        token: Token,
        #[help]
        help: String,
    },

    #[error("expected identifier")]
    #[diagnostic()]
    ExpectedIdentifier {
        #[label("expected an identifier here")]
        range: (usize, usize),
    },

    #[error("expected closing semicolon")]
    #[diagnostic()]
    ExpectedSemicolon {
        #[label("expected a semicolon here")]
        token: Token,
        #[help]
        help: String,
    },

    #[error("expected type")]
    #[diagnostic()]
    ExpectedType {
        #[label("expected a type here")]
        token: Token,
        #[help]
        help: String,
    },
}

#[derive(Clone, Error, Debug, Diagnostic)]
pub enum TypeCheckerError {
    #[error("mismatching types")]
    #[diagnostic()]
    TypeMismatch {
        #[label(collection, "")]
        labels: Vec<LabeledSpan>,

        #[help]
        help: String,
    },

    #[error("declaration not found")]
    #[diagnostic()]
    DeclarationNotFound {
        #[label(collection, "")]
        labels: Vec<LabeledSpan>,

        #[help]
        help: String,
    },

    #[error("missing parameter")]
    #[diagnostic()]
    MissingParameter {
        #[label("this parameter")]
        parameter: Parameter,

        #[label("expected parameter")]
        argument: (usize, usize),

        #[help]
        help: String,
    },

    #[error("missing field")]
    #[diagnostic()]
    MissingField {
        #[label("this field")]
        field: Field<TypedExpression>,

        #[label("expected field")]
        argument: (usize, usize),

        #[help]
        help: String,
    },

    #[error("unexpected argument")]
    #[diagnostic()]
    UnexpectedArgument {
        #[label("unexpected argument")]
        argument: TypedExpression,

        #[label("function signature")]
        function: FunctionType,

        #[help]
        help: String,
    },
}

pub fn lexer_unexpected_character(original: char, range: (usize, usize)) -> Error {
    Error::Lexer(LexerError::UnexpectedCharacter {
        help: format!("'{original}' cannot be parsed"),
        range,
    })
}

pub fn lexer_improper_number(original: &str, range: (usize, usize)) -> Error {
    Error::Lexer(LexerError::ImproperNumber {
        help: format!("'{original}' cannot be parsed as a number"),
        range,
    })
}

pub fn lexer_improper_character(original: &str, range: (usize, usize)) -> Error {
    Error::Lexer(LexerError::ImproperCharacter {
        help: format!("'{original}' cannot be parsed as a character"),
        range,
    })
}

pub fn parser_unexpected_token(
    help: impl Into<String>,
    token: &Token,
    expected: &TokenKind,
) -> Error {
    let help = help.into();

    Error::Parser(ParserError::UnexpectedToken {
        help: format!("{help}, but found {}", token.kind),
        token: token.clone(),
    })
}

pub fn parser_unexpected_end_of_file(span: (usize, usize), expected: impl Into<String>) -> Error {
    let expected = expected.into();

    Error::Parser(ParserError::UnexpectedEndOfFile {
        help: format!("expected {expected}, but reached the end of file"),
        labels: vec![LabeledSpan::new(
            Some(format!("expected {expected} here")),
            span.0,
            span.1,
        )],
    })
}

pub fn parser_expected_semicolon(token: &Token) -> Error {
    Error::Parser(ParserError::ExpectedSemicolon {
        help: format!("expected a semicolon after `{}`", token.value),
        token: token.clone(),
    })
}

pub fn parser_expected_expression(token: &Token) -> Error {
    Error::Parser(ParserError::ExpectedExpression {
        help: format!("{token} cannot be parsed as an expression"),
        token: token.clone(),
    })
}

pub fn parser_expected_identifier(span: impl Into<Span>) -> Error {
    let span = span.into();

    Error::Parser(ParserError::ExpectedIdentifier {
        range: (span.offset(), span.length()),
    })
}

pub fn parser_expected_type(token: &Token) -> Error {
    Error::Parser(ParserError::ExpectedType {
        help: format!("{token} cannot be parsed as a type"),
        token: token.clone(),
    })
}

pub fn type_checker_unexpected_type(
    expected: &Type,
    actual: &Type,
    expected_span: impl Into<Span>,
    help: impl Into<String>,
) -> Error {
    let expected_span = expected_span.into();

    Error::TypeChecker(TypeCheckerError::TypeMismatch {
        help: format!("expected {expected} but found {actual}, {}", help.into()),
        labels: vec![
            LabeledSpan::new(
                Some(format!("expected {expected}")),
                expected_span.offset(),
                expected_span.length(),
            ),
            LabeledSpan::new(
                Some(format!("passed {actual}")),
                actual.span.offset(),
                actual.span.length(),
            ),
        ],
    })
}

pub fn type_checker_unexpected_type_value(
    expected: &TypeValue,
    actual: &Type,
    help: impl Into<String>,
) -> Error {
    Error::TypeChecker(TypeCheckerError::TypeMismatch {
        help: format!("expected {expected} but found {actual}, {}", help.into()),
        labels: vec![LabeledSpan::new(
            Some(format!("{actual}")),
            actual.span.offset(),
            actual.span.length(),
        )],
    })
}

pub fn type_checker_type_mismatch(types: Vec<&Type>, help: impl Into<String>) -> Error {
    let distinct_types = types.iter().collect::<std::collections::HashSet<_>>();

    let most_occuring_type = if types.len() <= 2 {
        None
    } else {
        types
            .iter()
            .fold(std::collections::HashMap::new(), |mut acc, ty| {
                *acc.entry(&ty.value).or_insert(0) += 1;
                acc
            })
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(kind, _)| kind)
    };

    let invalid_types = match most_occuring_type {
        Some(most_occuring_type) => distinct_types
            .clone()
            .into_iter()
            .filter(|ty| ty.value != *most_occuring_type)
            .collect::<Vec<_>>(),
        None => distinct_types.clone().into_iter().collect::<Vec<_>>(),
    };

    let generated_help = match most_occuring_type {
        Some(most_occuring_type) => format!("this should probably be {most_occuring_type}"),
        None => format!("but {} were found", join_with_and(distinct_types)),
    };

    let labels: Vec<_> = match most_occuring_type {
        Some(_) => invalid_types
            .into_iter()
            .map(|ty| LabeledSpan::new(Some(format!("{ty}")), ty.span.offset(), ty.span.length()))
            .collect(),
        None => types
            .into_iter()
            .map(|ty| LabeledSpan::new(Some(format!("{ty}")), ty.span.offset(), ty.span.length()))
            .collect(),
    };

    Error::TypeChecker(TypeCheckerError::TypeMismatch {
        help: format!("{}, {generated_help}", help.into(),),
        labels,
    })
}

pub fn declaration_not_found(
    identifier: &Identifier,
    help: impl Into<String>,
    env: &TypeEnvironment,
) -> Error {
    let all_names: Vec<String> = env
        .get_all()
        .keys()
        .map(|ident| ident.name.to_string())
        .collect();

    let closest = closest_match(all_names, identifier.name.to_string());

    let help = if closest.is_none() {
        "no declarations found".to_string()
    } else {
        format!("did you mean `{}`?", closest.unwrap())
    };

    Error::TypeChecker(TypeCheckerError::DeclarationNotFound {
        help: format!("'{identifier}' was not found, {help}"),
        labels: vec![LabeledSpan::new(
            Some(format!("'{identifier}' is not declared")),
            identifier.span.offset(),
            identifier.span.length(),
        )],
    })
}

fn join_with_and<T, I>(items: I) -> String
where
    T: Display,
    I: IntoIterator<Item = T>,
{
    let items: Vec<_> = items.into_iter().collect();
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            if i == items.len() - 2 {
                format!("{item} and")
            } else if i == items.len() - 1 {
                format!("{item}")
            } else {
                format!("{item},")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn closest_match(haystack: Vec<String>, needle: String) -> Option<String> {
    // create matcher engine with default config
    let mut matcher = Matcher::new(Config::DEFAULT);
    // build a single-atom fuzzy pattern
    let pattern = Pattern::new(
        &needle,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    // for each item, compute score or default to zero
    let scored = haystack
        .iter()
        .map(|item| {
            let hay = nucleo_matcher::Utf32Str::Ascii(item.as_bytes());
            let score = pattern.score(hay, &mut matcher).unwrap_or(0);
            (item.clone(), score)
        })
        .collect::<Vec<_>>();

    if scored.is_empty() {
        None
    } else {
        scored
            .into_iter()
            .max_by_key(|(_, score)| *score)
            .map(|(item, _)| item)
    }
}

pub fn run(source: miette::NamedSource<String>) -> i64 {
    let lexer = Lexer::new(source.inner().as_str());

    let mut parser = Parser::new(lexer);
    let parsed = match parser.parse() {
        Ok(parsed) => parsed,
        Err(errors) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            std::process::exit(1);
        }
    };

    let mut type_checker = TypeChecker::new();
    let type_checked = match type_checker.check(&parsed) {
        Ok(typed_statement) => typed_statement,
        Err(errors) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            std::process::exit(1);
        }
    };

    let mut compiler = Compiler::new();
    let compiled = compiler.compile(&type_checked);

    let runner = Runner::new();
    let ran = runner.run(compiled).unwrap();

    ran
}
