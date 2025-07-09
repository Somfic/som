use std::path::PathBuf;

use crate::prelude::*;
use miette::LabeledSpan;
use parser::Parser;

mod compiler;
mod expressions;
mod lexer;
mod parser;
mod prelude;
mod runner;
mod statements;
mod type_checker;
mod types;

#[derive(clap::Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    file: PathBuf,
}

fn main() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .with_cause_chain()
                .context_lines(2)
                .build(),
        )
    }))
    .unwrap();

    let cli = <Cli as clap::Parser>::parse();

    let source = match std::fs::read_to_string(&cli.file) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("Error reading file {}: {}", cli.file.display(), err);
            std::process::exit(1);
        }
    };

    let lexer = Lexer::new(&source);

    let mut parser = Parser::new(lexer);
    let parsed = match parser.parse() {
        Ok(parsed) => parsed,
        Err(errors) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            std::process::exit(1);
        }
    };

    let mut type_checker = TypeChecker::new();
    let type_checked = match type_checker.check(&parsed) {
        Ok(typed_statement) => typed_statement,
        Err(errors) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            std::process::exit(1);
        }
    };

    // let labels = statement_to_labels(&parsed);
    // let report = miette::miette!(labels = labels.clone(), "found issues in this snippet")
    //     .with_source_code(source);
    // eprintln!("{:?}", report);

    let mut compiler = Compiler::new();
    let compiled = compiler.compile(&type_checked);

    let runner = Runner::new();
    let ran = runner.run(compiled).unwrap();

    println!("Result: {ran}");
}

fn statement_to_labels(statement: &Statement) -> Vec<LabeledSpan> {
    let mut labels = Vec::new();

    match &statement.value {
        StatementValue::Expression(expression) => {
            labels.push(statement.span.label("Expression"));
            labels.extend(expression_to_labels(expression));
        }
        StatementValue::Declaration(declaration) => {
            if let Some(explicit_type) = &declaration.explicit_type {
                labels.push(explicit_type.span.label("Explicit Type"));
            }
            labels.extend(expression_to_labels(&declaration.value));
        }
    };

    labels
}

fn expression_to_labels(expression: &Expression) -> Vec<LabeledSpan> {
    let mut labels = Vec::new();

    match &expression.value {
        ExpressionValue::Primary(primary) => match primary {
            PrimaryExpression::Integer(_) => labels.push(expression.span.label("Integer")),
            PrimaryExpression::Boolean(_) => labels.push(expression.span.label("Boolean")),
            PrimaryExpression::Unit => labels.push(expression.span.label("Unit")),
        },
        ExpressionValue::Binary(binary) => match binary.operator {
            BinaryOperator::Add => labels.push(binary.left.span.label("Add")),
            BinaryOperator::Subtract => labels.push(binary.left.span.label("Subtract")),
            BinaryOperator::Multiply => labels.push(binary.left.span.label("Multiply")),
            BinaryOperator::Divide => labels.push(binary.left.span.label("Divide")),
        },
        ExpressionValue::Call(_) => labels.push(expression.span.label("Call")),
        ExpressionValue::Group(group) => labels.extend(expression_to_labels(&group.expression)),
        ExpressionValue::Block(block) => {
            labels.extend(block.statements.iter().flat_map(statement_to_labels));
            labels.extend(expression_to_labels(&block.result));
        }
        ExpressionValue::Identifier(identifier) => {
            labels.push(identifier.span.label("Identifier"));
        }
        ExpressionValue::Function(function) => {
            labels.extend(function.parameters.iter().flat_map(parameter_to_labels));
            labels.extend(expression_to_labels(&function.body));
        }
    };

    labels
}

fn parameter_to_labels(parameter: &Parameter) -> Vec<LabeledSpan> {
    let mut labels = Vec::new();
    labels.push(parameter.identifier.span.label("Parameter Identifier"));
    labels.push(parameter.type_.span.label("Parameter Type"));
    labels
}
