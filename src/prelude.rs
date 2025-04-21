pub type ParserResult<T> = std::result::Result<T, Vec<miette::MietteDiagnostic>>;
pub type CompilerResult<T> = std::result::Result<T, Vec<miette::Report>>;

// pub trait Context {
//     fn context(self) -> Self;
// }

// impl<T> Context for ParserResult<T> {
//     fn context(self) -> Self {
//         self.map_err(|errors| {
//             errors.iter().map(|error| {
//             })
//         })
//     }
// }

//miette::miette!(error).wrap_err("lol");
