mod ast;
pub use ast::*;

use crate::type_check::TypeInferencer;
mod type_check;

fn main() {
    let mut ast = Ast::new();
    let mut inferencer = TypeInferencer::new();

    // Build: { let x = 12; let y = 13; x + y }

    // let x = 12
    let twelve = ast.alloc_expr(Expr::I32(12));
    let let_x = ast.alloc_stmt(Stmt::Let {
        name: Ident {
            id: 0,
            value: "x".into(),
        },
        ty: Some(Type::Bool),
        value: twelve,
    });

    // let y = 13
    let thirteen = ast.alloc_expr(Expr::I32(13));
    let let_y = ast.alloc_stmt(Stmt::Let {
        name: Ident {
            id: 1,
            value: "y".into(),
        },
        ty: None,
        value: thirteen,
    });

    // x + y
    let var_x = ast.alloc_expr(Expr::Var(Ident {
        id: 0,
        value: "x".into(),
    }));
    let var_y = ast.alloc_expr(Expr::Var(Ident {
        id: 1,
        value: "y".into(),
    }));
    let addition = ast.alloc_expr(Expr::Binary {
        op: BinOp::Add,
        lhs: var_x,
        rhs: var_y,
    });

    // Block
    let block = ast.alloc_expr(Expr::Block {
        stmts: vec![let_x, let_y],
        value: Some(addition),
    });

    let result_type = inferencer.infer(&ast, &block);

    println!("  Constraints:");
    for (i, constraint) in inferencer.constraints().iter().enumerate() {
        println!("    [{}] {:?}", i, constraint);
    }

    inferencer.solve(&ast).expect("Failed to solve constraints");

    let final_type = inferencer.normalize(&result_type);

    println!("  Final type: {:?}", final_type);
}
