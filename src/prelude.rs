pub type ParserResult<T> = std::result::Result<T, Vec<miette::MietteDiagnostic>>;
pub type CompilerResult<T> = std::result::Result<T, Vec<miette::Report>>;
