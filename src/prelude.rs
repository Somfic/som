use miette::{MietteDiagnostic, SourceCode};

pub type ParserResult<T> = std::result::Result<T, Diagnostics>;
pub type CompilerResult<T> = std::result::Result<T, Vec<miette::Report>>;

#[derive(Debug)]
pub struct Diagnostics(pub Vec<Diagnostic>);

impl Diagnostics {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn with(diagnostic: MietteDiagnostic) -> Self {
        Self(vec![Diagnostic::new(diagnostic)])
    }

    pub fn add(&mut self, diagnostic: MietteDiagnostic) {
        self.0.push(Diagnostic::new(diagnostic));
    }

    pub fn extend(&mut self, diagnostics: Diagnostics) {
        self.0.extend(diagnostics.0);
    }

    pub fn print(&self, source_code: impl SourceCode + 'static + Clone) {
        for diagnostic in &self.0 {
            let report = diagnostic.with_source_code(source_code.clone());
            eprintln!("{report:?}");
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        !self.is_empty()
    }
}

#[derive(Debug)]
pub struct Diagnostic {
    diagnostic: MietteDiagnostic,
    child: Option<Box<Diagnostic>>,
}

impl Diagnostic {
    pub fn new(diagnostic: MietteDiagnostic) -> Self {
        Self {
            diagnostic,
            child: None,
        }
    }

    pub fn context(&mut self, diagnostic: MietteDiagnostic) {
        if let Some(inner) = &mut self.child {
            inner.context(diagnostic);
        } else {
            self.child = Some(Box::new(Diagnostic::new(diagnostic)));
        }
    }

    pub fn with_source_code(&self, source_code: impl SourceCode + 'static) -> miette::Report {
        miette::miette!(self.diagnostic.clone()).with_source_code(source_code)
    }
}
