mod ast;
pub use ast::*;

use crate::type_check::TypeInferencer;
mod type_check;

fn main() {
    let mut ast = Ast::new();
    let mut inferencer = TypeInferencer::new();

    let lhs = ast.alloc_expr(Expr::I32(12));
    let rhs = ast.alloc_expr(Expr::I32(13));
    let addition = ast.alloc_expr(Expr::Binary {
        op: BinOp::Add,
        lhs,
        rhs,
    });

    let result_type = inferencer.infer(&ast, &addition);

    println!("  Constraints:");
    for (i, constraint) in inferencer.constraints().iter().enumerate() {
        println!("    [{}] {:?}", i, constraint);
    }

    inferencer.solve(&ast).expect("Failed to solve constraints");

    let final_type = inferencer.normalize(&result_type);

    println!("  Final type: {:?}", final_type);
}
