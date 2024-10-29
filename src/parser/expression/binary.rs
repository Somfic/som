use crate::parser::{
    ast::{BinaryOperator, Expression},
    lookup::BindingPower,
    Parser,
};
use miette::Result;

pub fn addition<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::Add,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}

pub fn multiplication<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::Multiply,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}

pub fn subtraction<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::Subtract,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}

pub fn division<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::Divide,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}
