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

use crate::expressions::TypedExpression;
use crate::lexer::{Identifier, Token, TokenKind};
use crate::type_checker::Environment as TypeEnvironment;
use crate::types::Type;

pub type Result<T> = std::result::Result<T, Error>;
pub type Results<T> = std::result::Result<T, Vec<Error>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Clone, Error, Debug, miette::Diagnostic)]
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

    #[error(transparent)]
    #[diagnostic(transparent)]
    Compiler(#[from] CompilerError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Runner(#[from] RunnerError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Module(crate::module::ModuleError),
}

#[derive(Clone, Error, Debug, miette::Diagnostic)]
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

    #[error("unterminated comment")]
    #[diagnostic()]
    UnterminatedComment {
        #[label("this comment was never closed")]
        range: (usize, usize),

        #[help]
        help: String,
    },
}

#[derive(Clone, Error, Debug, miette::Diagnostic)]
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

#[derive(Clone, Error, Debug, miette::Diagnostic)]
pub enum CompilerError {
    #[error("code generation failed")]
    #[diagnostic()]
    CodeGenerationFailed {
        #[label("failed to generate code for this expression")]
        span: Span,

        #[help]
        help: String,
    },

    #[error("unimplemented feature")]
    #[diagnostic()]
    Unimplemented {
        #[label("this feature is not yet implemented")]
        span: Span,

        #[help]
        help: String,
    },

    #[error("function finalization failed")]
    #[diagnostic()]
    FinalizationFailed {
        #[help]
        help: String,
    },
}

#[derive(Clone, Error, Debug, miette::Diagnostic)]
pub enum RunnerError {
    #[error("runtime trap occurred")]
    #[diagnostic()]
    RuntimeTrap {
        #[help]
        help: String,
    },

    #[error("runtime error")]
    #[diagnostic()]
    RuntimeError {
        #[help]
        help: String,
    },
}

#[derive(Clone, Error, Debug, miette::Diagnostic)]
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
        #[label("missing argument for this call")]
        argument: (usize, usize),

        #[help]
        help: String,
    },

    #[error("non existing field")]
    #[diagnostic()]
    UnknownField {
        #[label("this field does not exist")]
        field: Span,

        #[label("in this struct")]
        struct_span: Span,

        #[help]
        help: String,
    },

    #[error("missing required field")]
    #[diagnostic()]
    MissingRequiredField {
        #[label("missing field")]
        field: Span,

        #[label("in this constructor")]
        constructor: Span,

        #[help]
        help: String,
    },

    #[error("unexpected argument")]
    #[diagnostic()]
    UnexpectedArgument {
        #[label("unexpected argument")]
        argument: TypedExpression,

        #[label("signature")]
        signature: Span,

        #[help]
        help: String,
    },

    #[error("unknown extern function")]
    #[diagnostic()]
    UnknownExternFunction {
        #[label("unknown extern function")]
        function_span: Span,

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

pub fn lexer_unterminated_comment(range: (usize, usize)) -> Error {
    Error::Lexer(LexerError::UnterminatedComment {
        help: "Multi-line comments must be closed with '*/'".to_string(),
        range,
    })
}

pub fn parser_unexpected_token(
    help: impl Into<String>,
    token: &Token,
    _expected: &TokenKind,
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
    _help: impl Into<String>,
) -> Error {
    let _expected_span = expected_span.into();

    Error::TypeChecker(TypeCheckerError::TypeMismatch {
        help: format!("expected {expected} but found {actual}, {}", _help.into()),
        labels: vec![
            // Only show the actual (argument) span to avoid cross-file span issues
            LabeledSpan::new(
                Some(format!("passed {actual}, expected {expected}")),
                actual.span.offset(),
                actual.span.length(),
            ),
        ],
    })
}

pub fn type_checker_unexpected_type_value(
    expected: impl Into<String>,
    actual: &Type,
    _help: impl Into<String>,
) -> Error {
    Error::TypeChecker(TypeCheckerError::TypeMismatch {
        help: format!(
            "expected {} but found {actual}, {}",
            expected.into(),
            _help.into()
        ),
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
    _help: impl Into<String>,
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

pub fn closest_match(haystack: Vec<String>, needle: String) -> Option<String> {
    if haystack.is_empty() {
        return None;
    }

    // Create matcher engine with optimized config for better results
    let mut matcher = Matcher::new(Config::DEFAULT);

    // Build a fuzzy pattern with smart case matching
    let pattern = Pattern::new(
        &needle,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );

    // Score all candidates
    let mut scored: Vec<(String, u32)> = haystack
        .iter()
        .filter_map(|item| {
            let hay = nucleo_matcher::Utf32Str::Ascii(item.as_bytes());
            pattern
                .score(hay, &mut matcher)
                .map(|score| (item.clone(), score))
        })
        .collect();

    if scored.is_empty() {
        return None;
    }

    // Sort by score (highest first)
    scored.sort_by(|a, b| b.1.cmp(&a.1));

    let (best_match, best_score) = &scored[0];

    // Calculate similarity metrics for intelligent thresholding
    let needle_len = needle.len();
    let match_len = best_match.len();
    let length_diff = (needle_len as i32 - match_len as i32).abs() as usize;

    // Check if there are common characters
    let common_chars = needle.chars().filter(|c| best_match.contains(*c)).count();

    // Calculate a relative score (0.0 to 1.0)
    let relative_score = (*best_score as f64) / (needle_len.max(match_len) as f64 * 100.0);

    // Apply intelligent thresholds based on different criteria:

    // 1. High fuzzy score threshold (good match quality)
    if relative_score >= 0.6 {
        return Some(best_match.clone());
    }

    // 2. Prefix matching (starts with same characters)
    if best_match
        .to_lowercase()
        .starts_with(&needle.to_lowercase())
        || needle
            .to_lowercase()
            .starts_with(&best_match.to_lowercase())
    {
        return Some(best_match.clone());
    }

    // 3. Good character overlap with reasonable length difference
    if common_chars >= needle_len.min(3) && length_diff <= 2 {
        return Some(best_match.clone());
    }

    // 4. Substring matching (one contains the other)
    if best_match.to_lowercase().contains(&needle.to_lowercase())
        || needle.to_lowercase().contains(&best_match.to_lowercase())
    {
        return Some(best_match.clone());
    }

    // 5. For very short needles, be more lenient
    if needle_len <= 3 && common_chars >= needle_len / 2 && length_diff <= 3 {
        return Some(best_match.clone());
    }

    // 6. For longer needles, require better relative score
    if needle_len > 6 && relative_score >= 0.3 && common_chars >= needle_len / 3 {
        return Some(best_match.clone());
    }

    // If none of the criteria are met, don't suggest anything
    None
}
