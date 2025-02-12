use std::borrow::Cow;

pub struct Tokenizer<'a> {
    source_code: Cow<'a, &'a str>,
    remainder: &'a str,
    byte_offset: usize,
    peeked: Option<Token<'a>>,
}

impl<'a> Tokenizer<'a> {
    pub fn new(source_code: Cow<'a, &'a str>) -> Tokenizer<'a> {
        Tokenizer { source_code }
    }
}
