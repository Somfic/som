use strum::Display;

pub struct Node {
    pub tag: Tag,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[strum(serialize_all = "lowercase")]
pub enum Tag {
    Main,
    Block,
    Button,
    Text,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[strum(serialize_all = "lowercase")]
pub enum Event {
    Click,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[strum(serialize_all = "lowercase")]
pub enum Attribute {
    Style,
}

pub struct Handler;
