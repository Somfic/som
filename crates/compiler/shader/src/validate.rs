//! Each shader's WGSL, read and checked by naga, wgpu's own shader compiler, while compiling:
//! WGSL that wgpu would refuse when the program runs is a bug here, said before it runs.

/// What naga finds wrong with `wgsl`, if anything.
pub(crate) struct Invalid {
    /// naga's words.
    pub words: String,
    /// The line of `wgsl` it's on, numbered from 1, and its text.
    pub line: Option<(usize, String)>,
}

pub(crate) fn validate(wgsl: &str) -> Result<(), Invalid> {
    let module = naga::front::wgsl::parse_str(wgsl)
        .map_err(|e| invalid(e.message().to_string(), e.location(wgsl), wgsl))?;
    let flags = naga::valid::ValidationFlags::all();
    let mut validator = naga::valid::Validator::new(flags, naga::valid::Capabilities::default());
    validator.validate(&module).map_err(|e| {
        // The error and each cause under it: "Function [3] 'f' is invalid", "Expression [7] …".
        let mut words = e.to_string();
        let mut cause = std::error::Error::source(e.as_inner());
        while let Some(c) = cause {
            words.push_str(": ");
            words.push_str(&c.to_string());
            cause = c.source();
        }
        invalid(words, e.location(wgsl), wgsl)
    })?;
    Ok(())
}

fn invalid(words: String, at: Option<naga::SourceLocation>, wgsl: &str) -> Invalid {
    let line = at.and_then(|at| {
        let n = at.line_number as usize;
        wgsl.lines().nth(n.checked_sub(1)?).map(|text| (n, text.trim().to_string()))
    });
    Invalid { words, line }
}
