use strum::Display;

pub struct Node {
    pub(crate) tag: Tag,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[strum(serialize_all = "lowercase")]
pub enum Tag {
    Main,
    Block,
    Text,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
#[strum(serialize_all = "lowercase")]
pub enum Event {
    Click,
}

pub struct Handler;
