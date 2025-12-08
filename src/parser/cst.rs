use crate::ast::*;
use crate::lexer::{Syntax, SyntaxNode};
use crate::span::Span;
use rowan::GreenNode;

fn node_span(node: &SyntaxNode) -> Span {
    let range = node.text_range();
    Span::new(range.start().into(), range.end().into())
}

pub fn to_ast(green: GreenNode) -> Ast {
    let root = SyntaxNode::new_root(green);
    let mut ast = Ast::new();

    for child in root.children() {
        if child.kind() == Syntax::FuncDec {
            convert_func_dec(&mut ast, child);
        }
    }

    ast
}

fn convert_func_dec(ast: &mut Ast, node: SyntaxNode) -> FuncId {
    let span = node_span(&node);
    let mut name = None;
    let mut parameters = Vec::new();
    let mut return_type = None;
    let mut return_type_id = None;
    let mut body = None;

    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Token(token) => {
                if token.kind() == Syntax::Ident && name.is_none() {
                    name = Some(Ident {
                        id: 0, // Will be assigned properly later if needed
                        value: token.text().to_string().into_boxed_str(),
                    });
                }
            }
            rowan::NodeOrToken::Node(child_node) => match child_node.kind() {
                Syntax::FuncParam => {
                    parameters.push(convert_func_param(ast, child_node));
                }
                Syntax::TypeAnnotation => {
                    if return_type.is_none() {
                        let type_span = node_span(&child_node);
                        let type_id = ast.alloc_type_with_span(type_span);
                        return_type = Some(convert_type_annotation(ast, child_node));
                        return_type_id = Some(type_id);
                    }
                }
                Syntax::Block => {
                    body = Some(convert_block(ast, child_node));
                }
                _ => {}
            },
        }
    }

    let func = FuncDec {
        name: name.unwrap_or(Ident {
            id: 0,
            value: "".into(),
        }),
        parameters,
        return_type,
        return_type_id,
        body: body.unwrap_or_else(|| ast.alloc_expr(Expr::Hole)),
    };

    ast.alloc_func_with_span(func, span)
}

fn convert_func_param(_ast: &mut Ast, node: SyntaxNode) -> FuncParam {
    let mut name = None;
    let mut ty = None;

    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Token(token) => {
                if token.kind() == Syntax::Ident && name.is_none() {
                    name = Some(Ident {
                        id: 0,
                        value: token.text().to_string().into_boxed_str(),
                    });
                }
            }
            rowan::NodeOrToken::Node(child_node) => {
                if child_node.kind() == Syntax::TypeAnnotation {
                    ty = Some(convert_type_annotation(_ast, child_node));
                }
            }
        }
    }

    FuncParam {
        name: name.unwrap_or(Ident {
            id: 0,
            value: "".into(),
        }),
        ty,
    }
}

fn convert_type_annotation(_ast: &mut Ast, node: SyntaxNode) -> Type {
    for child in node.children_with_tokens() {
        if let rowan::NodeOrToken::Token(token) = child {
            if token.kind() == Syntax::Ident {
                let type_name = token.text().to_string();
                return match type_name.as_str() {
                    "i32" => Type::I32,
                    "bool" => Type::Bool,
                    "unit" => Type::Unit,
                    _ => Type::Unit, // Default for unknown types
                };
            }
        }
    }

    Type::Unit
}

fn convert_block(ast: &mut Ast, node: SyntaxNode) -> ExprId {
    let span = node_span(&node);
    let mut stmts = Vec::new();
    let mut value = None;

    let children: Vec<_> = node.children().collect();

    for (i, child) in children.iter().enumerate() {
        let is_last = i == children.len() - 1;

        match child.kind() {
            Syntax::LetStmt => {
                stmts.push(convert_let_stmt(ast, child.clone()));
            }
            Syntax::ExprStmt => {
                let expr = convert_expr_stmt(ast, child.clone());
                if is_last {
                    // Last expression becomes the block value
                    value = Some(expr);
                } else {
                    // Not last, so it's a statement (we ignore for now)
                }
            }
            _ => {}
        }
    }

    ast.alloc_expr_with_span(Expr::Block { stmts, value }, span)
}

fn convert_let_stmt(ast: &mut Ast, node: SyntaxNode) -> StmtId {
    let span = node_span(&node);
    let mut name = None;
    let mut ty = None;
    let mut value = None;

    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Token(token) => {
                if token.kind() == Syntax::Ident && name.is_none() {
                    name = Some(Ident {
                        id: 0,
                        value: token.text().to_string().into_boxed_str(),
                    });
                }
            }
            rowan::NodeOrToken::Node(child_node) => match child_node.kind() {
                Syntax::TypeAnnotation => {
                    ty = Some(convert_type_annotation(ast, child_node));
                }
                Syntax::VarExpr
                | Syntax::IntExpr
                | Syntax::BinaryExpr
                | Syntax::CallExpr
                | Syntax::ParenExpr
                | Syntax::Block => {
                    if value.is_none() {
                        value = Some(convert_expr(ast, child_node));
                    }
                }
                _ => {}
            },
        }
    }

    let stmt = Stmt::Let {
        name: name.unwrap_or(Ident {
            id: 0,
            value: "".into(),
        }),
        ty,
        value: value.unwrap_or_else(|| ast.alloc_expr(Expr::Hole)),
    };

    ast.alloc_stmt_with_span(stmt, span)
}

fn convert_expr_stmt(ast: &mut Ast, node: SyntaxNode) -> ExprId {
    if let Some(child) = node.children().next() {
        return convert_expr(ast, child);
    }

    ast.alloc_expr(Expr::Hole)
}

fn convert_expr(ast: &mut Ast, node: SyntaxNode) -> ExprId {
    match node.kind() {
        Syntax::VarExpr => convert_var_expr(ast, node),
        Syntax::IntExpr => convert_int_expr(ast, node),
        Syntax::BinaryExpr => convert_binary_expr(ast, node),
        Syntax::CallExpr => convert_call_expr(ast, node),
        Syntax::ParenExpr => convert_paren_expr(ast, node),
        Syntax::Block => convert_block(ast, node),
        Syntax::Error => ast.alloc_expr(Expr::Hole),
        _ => ast.alloc_expr(Expr::Hole),
    }
}

fn convert_var_expr(ast: &mut Ast, node: SyntaxNode) -> ExprId {
    // Check tokens directly to get precise span
    for token in node.children_with_tokens() {
        if let rowan::NodeOrToken::Token(token) = token {
            if token.kind() == Syntax::Ident {
                let token_range = token.text_range();
                let token_span = Span::new(token_range.start().into(), token_range.end().into());
                return ast.alloc_expr_with_span(
                    Expr::Var(Ident {
                        id: 0,
                        value: token.text().to_string().into_boxed_str(),
                    }),
                    token_span,
                );
            }
        }
    }

    // Check child nodes
    for child in node.children() {
        if child.kind() == Syntax::Ident {
            let child_span = node_span(&child);
            return ast.alloc_expr_with_span(
                Expr::Var(Ident {
                    id: 0,
                    value: child.text().to_string().into_boxed_str(),
                }),
                child_span,
            );
        }
    }

    ast.alloc_expr_with_span(Expr::Hole, node_span(&node))
}

fn convert_int_expr(ast: &mut Ast, node: SyntaxNode) -> ExprId {
    for token in node.children_with_tokens() {
        if let rowan::NodeOrToken::Token(token) = token {
            if token.kind() == Syntax::Int {
                let token_range = token.text_range();
                let token_span = Span::new(token_range.start().into(), token_range.end().into());
                let value = token.text().parse::<i32>().unwrap_or(0);
                return ast.alloc_expr_with_span(Expr::I32(value), token_span);
            }
        }
    }

    ast.alloc_expr_with_span(Expr::Hole, node_span(&node))
}

fn convert_binary_expr(ast: &mut Ast, node: SyntaxNode) -> ExprId {
    let span = node_span(&node);
    let mut lhs = None;
    let mut op = None;
    let mut rhs = None;

    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(child_node) => {
                if lhs.is_none() {
                    lhs = Some(convert_expr(ast, child_node));
                } else if rhs.is_none() {
                    rhs = Some(convert_expr(ast, child_node));
                }
            }
            rowan::NodeOrToken::Token(token) => {
                if op.is_none() {
                    op = match token.kind() {
                        Syntax::Plus => Some(BinOp::Add),
                        Syntax::Minus => Some(BinOp::Subtract),
                        Syntax::Star => Some(BinOp::Multiply),
                        Syntax::Slash => Some(BinOp::Divide),
                        Syntax::EqEq => Some(BinOp::Equals),
                        Syntax::NotEq => Some(BinOp::NotEquals),
                        Syntax::Lt => Some(BinOp::LessThan),
                        Syntax::Gt => Some(BinOp::GreaterThan),
                        _ => None,
                    };
                }
            }
        }
    }

    if let (Some(lhs), Some(op), Some(rhs)) = (lhs, op, rhs) {
        ast.alloc_expr_with_span(Expr::Binary { op, lhs, rhs }, span)
    } else {
        ast.alloc_expr_with_span(Expr::Hole, span)
    }
}

fn convert_call_expr(ast: &mut Ast, node: SyntaxNode) -> ExprId {
    let span = node_span(&node);
    let mut func_name = None;
    let mut args = Vec::new();

    for child in node.children_with_tokens() {
        if let rowan::NodeOrToken::Node(child_node) = child {
            match child_node.kind() {
                Syntax::VarExpr => {
                    if func_name.is_none() {
                        // Get the function name
                        for token in child_node.children_with_tokens() {
                            if let rowan::NodeOrToken::Token(token) = token {
                                if token.kind() == Syntax::Ident {
                                    func_name = Some(token.text().to_string());
                                    break;
                                }
                            }
                        }
                    } else {
                        // Already have func name, this is an argument
                        args.push(convert_expr(ast, child_node));
                    }
                }
                Syntax::IntExpr
                | Syntax::BinaryExpr
                | Syntax::CallExpr
                | Syntax::ParenExpr
                | Syntax::Block => {
                    args.push(convert_expr(ast, child_node));
                }
                _ => {}
            }
        }
    }

    // Find the function by name
    if let Some(name) = func_name {
        for (i, func) in ast.funcs.iter().enumerate() {
            if func.name.value.as_ref() == name {
                return ast.alloc_expr_with_span(
                    Expr::Call {
                        func: FuncId(i as u32),
                        args,
                    },
                    span,
                );
            }
        }
    }

    // Function not found, return hole
    ast.alloc_expr_with_span(Expr::Hole, span)
}

fn convert_paren_expr(ast: &mut Ast, node: SyntaxNode) -> ExprId {
    if let Some(child) = node.children().next() {
        return convert_expr(ast, child);
    }

    ast.alloc_expr(Expr::Hole)
}
