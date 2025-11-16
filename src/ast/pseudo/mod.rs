mod expression;
mod statement;

pub trait Pseudo {
    fn pseudo(&self) -> String;
}
