mod ast;
pub use ast::*;

use crate::type_check::TypeInferencer;
mod type_check;

fn main() {
    let mut ast = Ast::new();
    let inferencer = TypeInferencer::new();

    // Build a function: fn add(x: i32, y: i32) { x + y }
    let param_x = ast.alloc_expr(Expr::Var(Ident {
        id: 0,
        value: "x".into(),
    }));
    let param_y = ast.alloc_expr(Expr::Var(Ident {
        id: 1,
        value: "y".into(),
    }));
    let add_body = ast.alloc_expr(Expr::Binary {
        op: BinOp::LessThan,
        lhs: param_x,
        rhs: param_y,
    });

    let add_func = ast.alloc_func(FuncDec {
        name: Ident {
            id: 0,
            value: "add".into(),
        },
        parameters: vec![
            FuncParam {
                name: Ident {
                    id: 1,
                    value: "x".into(),
                },
                ty: Some(Type::I32),
            },
            FuncParam {
                name: Ident {
                    id: 2,
                    value: "y".into(),
                },
                ty: Some(Type::I32),
            },
        ],
        return_type: None,
        body: add_body,
    });

    // Build a call: add(5, 10)
    let five = ast.alloc_expr(Expr::I32(5));
    let ten = ast.alloc_expr(Expr::I32(10));
    let call_add = ast.alloc_expr(Expr::Call {
        func: add_func,
        args: vec![five, ten],
    });

    // Build a function that calls add: fn main() { add(5, 10) }
    let main_func = ast.alloc_func(FuncDec {
        name: Ident {
            id: 3,
            value: "main".into(),
        },
        parameters: vec![],
        return_type: None,
        body: call_add,
    });

    // Type check the entire program
    println!("Type checking program...");
    let typed_ast = inferencer.check_program(ast);

    println!("Constraints");
    for constraint in &typed_ast.constraints {
        println!("  {:?}", constraint);
    }

    println!("\nExpression types:");
    for (expr_id, ty) in &typed_ast.types {
        println!("  {:?} → {:?}", expr_id, ty);
    }

    println!("\nCall result type: {:?}", typed_ast.types.get(&call_add));

    if typed_ast.errors.is_empty() {
        println!("\n✓ No errors!");
    } else {
        println!("\n✗ Errors found:");
        for (expr_id, error) in &typed_ast.errors {
            println!("  At {:?}: {:?}", expr_id, error);
        }
    }
}
