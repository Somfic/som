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

pub fn modulo<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::Modulo,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}

pub fn equal<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::Equal,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}

pub fn not_equal<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::NotEqual,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}

pub fn less_than<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::LessThan,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}

pub fn less_than_or_equal<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::LessThanOrEqual,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}

pub fn greater_than<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::GreaterThan,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}

pub fn greater_than_or_equal<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::GreaterThanOrEqual,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}

pub fn and<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::And,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}

pub fn or<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::Binary {
        operator: BinaryOperator::Or,
        left: Box::new(lhs),
        right: Box::new(rhs),
    })
}
