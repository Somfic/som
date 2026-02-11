mod error;
use crate::{
    Expr, Lifetime, Stmt, Type, TypedAst,
    arena::{Arena, Id},
    scope::ScopedEnvironment,
};
pub use error::*;
use std::collections::HashMap;

pub struct Place {
    name: String,
    scope_depth: u32,
}

#[derive(Clone)]
pub enum State {
    Owned,
    Moved { to: Id<Expr> },
    Borrowed { loans: Vec<Id<Loan>> },
    MutBorrowed { loan: Id<Loan> },
}

#[derive(Clone)]
pub enum ReferenceOrigin {
    Local {
        at: Id<Place>,
        scope_depth: u32,
        borrow_expr: Id<Expr>,
    },
    Parameter,
    Static,
}

pub struct Loan {
    at: Id<Place>,
    mutable: bool,
    expr: Id<Expr>,
    scope_depth: u32,
}

pub struct BorrowChecker<'a> {
    typed_ast: &'a TypedAst,                         // The input AST with types
    places: Arena<Place>,                            // All places
    name_to_place: ScopedEnvironment<Id<Place>>,     // Lookup by name
    place_states: HashMap<Id<Place>, State>,         // Current state
    ref_origins: HashMap<Id<Expr>, ReferenceOrigin>, // Where refs point
    place_origins: HashMap<Id<Place>, ReferenceOrigin>, // Where place refs point
    loans: Arena<Loan>,                              // All loans created
    scope_depth: u32,                                // Current nesting level
    errors: Vec<BorrowError>,                        // Collected errors
}

impl<'a> BorrowChecker<'a> {
    pub fn new(typed_ast: &'a TypedAst) -> Self {
        Self {
            typed_ast,
            places: Arena::new(),
            name_to_place: ScopedEnvironment::new(),
            place_states: HashMap::new(),
            ref_origins: HashMap::new(),
            place_origins: HashMap::new(),
            loans: Arena::new(),
            scope_depth: 0,
            errors: vec![],
        }
    }

    pub fn check_program(&mut self) -> Vec<BorrowError> {
        for func in &self.typed_ast.ast.funcs {
            // reset state
            self.places = Arena::new();
            self.name_to_place = ScopedEnvironment::new();
            self.place_states = HashMap::new();
            self.ref_origins = HashMap::new();
            self.place_origins = HashMap::new();
            self.loans = Arena::new();
            self.scope_depth = 0;

            for param in &func.parameters {
                self.fresh_place(param.name.value.to_string());
            }

            self.check_expr(func.body);
        }

        self.errors.clone()
    }

    /// checks if a variable can be read
    fn check_read(&mut self, name: impl Into<String>, expr_id: Id<Expr>) {
        let name = name.into();

        let place_id = match self.name_to_place.get(&name) {
            Some(place_id) => place_id,
            None => return,
        };

        let state = match self.place_states.get(place_id) {
            Some(state) => state,
            None => return,
        };

        let error = match state {
            State::Owned => return,
            State::Moved { to } => BorrowError::UseAfterMove {
                name,
                use_expr: expr_id,
                moved_at: *to,
            },
            State::Borrowed { .. } => return,
            State::MutBorrowed { loan } => {
                let loan = self.loans.get(loan);
                BorrowError::UseWhileMutBorrowed {
                    name,
                    use_expr: expr_id,
                    borrow_expr: loan.expr,
                }
            }
        };

        self.errors.push(error);
    }

    fn check_move(&mut self, expr_id: Id<Expr>) {
        // copy types are just copied, no need to check moves
        if let Some(ty) = self.typed_ast.types.get(&expr_id)
            && self.is_copy(ty)
        {
            self.check_expr(expr_id);
            return;
        }

        let expr = self.typed_ast.ast.exprs.get(&expr_id);

        // for non-variables, just check the expression
        let Expr::Var(name) = expr else {
            self.check_expr(expr_id);
            return;
        };

        // check if readable
        self.check_read(name.value.clone(), expr_id);

        // find the place and state
        let Some(&place_id) = self.name_to_place.get(&*name.value) else {
            return;
        };
        let Some(state) = self.place_states.get(&place_id).cloned() else {
            return;
        };

        // try moving
        match state {
            State::Owned => {
                // mark as moved
                self.place_states
                    .insert(place_id, State::Moved { to: expr_id });
            }
            State::Moved { .. } => { /* already reported */ }
            State::Borrowed { loans } => {
                // report error
                let loan = self.loans.get(&loans[0]);
                self.errors.push(BorrowError::MoveWhileBorrowed {
                    name: name.value.to_string(),
                    move_expr: expr_id,
                    borrow_expr: loan.expr,
                });
            }
            State::MutBorrowed { loan } => {
                // report error
                let loan_data = self.loans.get(&loan);
                self.errors.push(BorrowError::MoveWhileBorrowed {
                    name: name.value.to_string(),
                    move_expr: expr_id,
                    borrow_expr: loan_data.expr,
                });
            }
        }
    }

    fn check_borrow(&mut self, mutable: bool, inner_expr: Id<Expr>, borrow_expr: Id<Expr>) {
        let inner = self.typed_ast.ast.exprs.get(&inner_expr);

        // for non-variables, check the expression
        let Expr::Var(name) = inner else {
            self.check_expr(inner_expr);
            return;
        };

        // find the place and state
        let Some(&place_id) = self.name_to_place.get(&*name.value) else {
            return;
        };
        let Some(state) = self.place_states.get(&place_id).cloned() else {
            return;
        };

        // record origin for dangling reference checks
        let place = self.places.get(&place_id);
        let origin = if place.scope_depth == 0 {
            ReferenceOrigin::Parameter
        } else {
            ReferenceOrigin::Local {
                at: place_id,
                scope_depth: place.scope_depth,
                borrow_expr,
            }
        };
        self.ref_origins.insert(borrow_expr, origin);

        // check for conflicts
        match state {
            State::Moved { to } => {
                // used after already moved
                self.errors.push(BorrowError::UseAfterMove {
                    name: name.value.to_string(),
                    use_expr: borrow_expr,
                    moved_at: to,
                });
            }
            State::MutBorrowed { loan } => {
                // already borrowed
                let existing = self.loans.get(&loan);
                self.errors.push(BorrowError::ConflictingBorrow {
                    name: name.value.to_string(),
                    new_expr: borrow_expr,
                    new_mut: mutable,
                    existing_expr: existing.expr,
                    existing_mut: true,
                });
            }
            State::Borrowed { ref loans } => {
                if mutable {
                    // can't borrow mutably while immutably borrowed
                    let existing = self.loans.get(&loans[0]);
                    self.errors.push(BorrowError::ConflictingBorrow {
                        name: name.value.to_string(),
                        new_expr: borrow_expr,
                        new_mut: true,
                        existing_expr: existing.expr,
                        existing_mut: false,
                    });
                } else {
                    // another immutable borrow is OK
                    let loan_id = self.fresh_loan(place_id, false, borrow_expr);
                    let mut new_loans = loans.clone();
                    new_loans.push(loan_id);
                    self.place_states
                        .insert(place_id, State::Borrowed { loans: new_loans });
                }
            }
            State::Owned => {
                // fresh borrow
                let loan_id = self.fresh_loan(place_id, mutable, borrow_expr);
                if mutable {
                    self.place_states
                        .insert(place_id, State::MutBorrowed { loan: loan_id });
                } else {
                    self.place_states.insert(
                        place_id,
                        State::Borrowed {
                            loans: vec![loan_id],
                        },
                    );
                }
            }
        }
    }

    fn check_expr(&mut self, expr_id: Id<Expr>) {
        let expr = self.typed_ast.ast.exprs.get(&expr_id);

        match expr {
            Expr::Hole | Expr::I32(_) | Expr::Bool(_) | Expr::String(_) => {
                // Literals - nothing to check
            }
            Expr::Var(name) => {
                self.check_read(&*name.value, expr_id);
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr(*lhs);
                self.check_expr(*rhs);
            }
            Expr::Block { stmts, value } => {
                self.push_scope();

                for stmt_id in stmts {
                    self.check_stmt(*stmt_id);
                }

                if let Some(val) = value {
                    self.check_expr(*val);
                    let ty = self.typed_ast.types.get(val);

                    // Only check for dangling if return type is a reference
                    if !matches!(ty, Some(Type::Reference { .. })) {
                        self.pop_scope();
                        return;
                    }

                    // 'static references never dangle
                    if matches!(
                        ty,
                        Some(Type::Reference {
                            lifetime: Lifetime::Static,
                            ..
                        })
                    ) {
                        self.pop_scope();
                        return;
                    }

                    // Try to find origin: first from direct borrow, then from variable
                    let origin = self.ref_origins.get(val).cloned().or_else(|| {
                        // If val is a variable, look up its place's origin
                        let val_expr = self.typed_ast.ast.exprs.get(val);
                        if let Expr::Var(name) = val_expr {
                            let place_id = self.name_to_place.get(&*name.value)?;
                            self.place_origins.get(place_id).cloned()
                        } else {
                            None
                        }
                    });

                    // Propagate origin from return value to the block expression
                    if let Some(ref origin) = origin {
                        self.ref_origins.insert(expr_id, origin.clone());
                    }

                    if let Some(ReferenceOrigin::Local {
                        at,
                        scope_depth,
                        borrow_expr,
                    }) = origin
                        && scope_depth == self.scope_depth
                    {
                        let place = self.places.get(&at);
                        self.errors.push(BorrowError::DanglingReference {
                            name: place.name.clone(),
                            borrow_expr,
                            return_expr: *val,
                        });
                    }
                }

                self.pop_scope();
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    self.check_move(*arg);
                }
            }
            Expr::Borrow { mutable, expr } => {
                self.check_borrow(*mutable, *expr, expr_id);
            }
            Expr::Deref { expr } => {
                self.check_expr(*expr);
            }
            Expr::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                self.check_expr(*condition);
                self.check_expr(*truthy);
                self.check_expr(*falsy);
            }
        }
    }

    fn check_stmt(&mut self, stmt_id: Id<Stmt>) {
        let stmt = self.typed_ast.ast.stmts.get(&stmt_id);

        match stmt {
            Stmt::Let { name, value, .. } => {
                // move the RHS value
                self.check_move(*value);

                // create a place for the new binding
                let place_id = self.fresh_place(&*name.value);

                // propagate reference origin if the value is a reference
                if let Some(origin) = self.ref_origins.get(value).cloned() {
                    self.place_origins.insert(place_id, origin);
                }
            }
            Stmt::Expr { expr } => {
                self.check_expr(*expr);
            }
        }
    }

    fn fresh_place(&mut self, name: impl Into<String>) -> Id<Place> {
        let name = name.into();
        let place = Place {
            name: name.clone(),
            scope_depth: self.scope_depth,
        };

        let id = self.places.alloc(place);

        self.name_to_place.insert(name, id);

        self.place_states.insert(id, State::Owned);

        id
    }

    fn fresh_loan(&mut self, at: Id<Place>, mutable: bool, expr: Id<Expr>) -> Id<Loan> {
        let loan = Loan {
            at,
            expr,
            mutable,
            scope_depth: self.scope_depth,
        };

        self.loans.alloc(loan)
    }

    fn push_scope(&mut self) {
        self.scope_depth += 1;
        self.name_to_place.enter_scope();
    }

    fn pop_scope(&mut self) {
        // find loans that expire
        let loans = self
            .loans
            .iter()
            .enumerate()
            .filter(|(_, loan)| loan.scope_depth == self.scope_depth)
            .map(|(idx, _)| Id::new(idx));

        // process expiring loans
        for loan_id in loans {
            let loan = self.loans.get(&loan_id);
            let place_id = loan.at;

            if let Some(state) = self.place_states.get_mut(&place_id) {
                match state {
                    State::MutBorrowed { loan } if *loan == loan_id => {
                        *state = State::Owned;
                    }
                    State::Borrowed { loans } => {
                        loans.retain(|id| *id != loan_id);
                        if loans.is_empty() {
                            *state = State::Owned;
                        }
                    }
                    _ => {}
                }
            }
        }

        self.name_to_place.leave_scope();

        self.scope_depth -= 1;
    }

    fn is_copy(&self, ty: &Type) -> bool {
        matches!(
            ty,
            Type::Unit | Type::Bool | Type::I32 | Type::Reference { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Source, parser, type_check::TypeInferencer};
    use std::sync::Arc;

    fn check(source: &str) -> Vec<BorrowError> {
        let source = Arc::new(Source::from_raw(source));
        let (ast, _) = parser::parse(source);
        let inferencer = TypeInferencer::new();
        let typed_ast = inferencer.check_program(ast);
        let mut checker = BorrowChecker::new(&typed_ast);
        checker.check_program()
    }

    fn has_error<F: Fn(&BorrowError) -> bool>(errors: &[BorrowError], predicate: F) -> bool {
        errors.iter().any(predicate)
    }

    #[test]
    fn test_use_after_move() {
        let errors = check(
            r#"
            fn test() {
                let x = 10;
                let y = x;
                let z = x;
            }
            "#,
        );
        // i32 is Copy, so no error expected
        assert!(errors.is_empty());
    }

    #[test]
    fn test_conflicting_borrow_mut_then_immut() {
        let errors = check(
            r#"
            fn test() {
                let x = 10;
                let r = &mut x;
                let s = &x;
            }
            "#,
        );
        assert!(has_error(&errors, |e| matches!(
            e,
            BorrowError::ConflictingBorrow { .. }
        )));
    }

    #[test]
    fn test_conflicting_borrow_immut_then_mut() {
        let errors = check(
            r#"
            fn test() {
                let x = 10;
                let r = &x;
                let s = &mut x;
            }
            "#,
        );
        assert!(has_error(&errors, |e| matches!(
            e,
            BorrowError::ConflictingBorrow { .. }
        )));
    }

    #[test]
    fn test_multiple_immutable_borrows_ok() {
        let errors = check(
            r#"
            fn test() {
                let x = 10;
                let r = &x;
                let s = &x;
            }
            "#,
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_dangling_reference_direct() {
        let errors = check(
            r#"
            fn test() -> &i32 {
                let x = 10;
                &x
            }
            "#,
        );
        assert!(has_error(&errors, |e| matches!(
            e,
            BorrowError::DanglingReference { .. }
        )));
    }

    #[test]
    fn test_dangling_reference_through_variable() {
        let errors = check(
            r#"
            fn test() -> &i32 {
                let x = 10;
                let r = &x;
                r
            }
            "#,
        );
        assert!(has_error(&errors, |e| matches!(
            e,
            BorrowError::DanglingReference { .. }
        )));
    }

    #[test]
    fn test_parameter_reference_ok() {
        let errors = check(
            r#"
            fn test(x: &i32) -> &i32 {
                x
            }
            "#,
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_borrow_parameter_ok() {
        let errors = check(
            r#"
            fn test(x: i32) -> &i32 {
                &x
            }
            "#,
        );
        // x is a parameter at scope 0, so &x should be ok to return
        assert!(errors.is_empty());
    }

    #[test]
    fn test_borrow_expires_after_scope() {
        let errors = check(
            r#"
            fn test() {
                let x = 10;
                {
                    let r = &mut x;
                }
                let s = &mut x;
            }
            "#,
        );
        // First borrow expires when inner block ends, second borrow is ok
        assert!(errors.is_empty());
    }

    #[test]
    fn test_use_while_mut_borrowed() {
        let errors = check(
            r#"
            fn test() {
                let x = 10;
                let r = &mut x;
                let y = x + 1;
            }
            "#,
        );
        assert!(has_error(&errors, |e| matches!(
            e,
            BorrowError::UseWhileMutBorrowed { .. }
        )));
    }

    #[test]
    fn test_nested_block_dangling() {
        let errors = check(
            r#"
            fn test() -> &i32 {
                let r = {
                    let x = 10;
                    &x
                };
                r
            }
            "#,
        );
        assert!(has_error(&errors, |e| matches!(
            e,
            BorrowError::DanglingReference { .. }
        )));
    }

    #[test]
    fn test_outer_scope_reference_ok() {
        let errors = check(
            r#"
            fn test() -> &i32 {
                let x = 10;
                let r = {
                    &x
                };
                r
            }
            "#,
        );
        // x is in outer scope, so returning &x from inner block is ok
        // But returning from function is still dangling!
        assert!(has_error(&errors, |e| matches!(
            e,
            BorrowError::DanglingReference { .. }
        )));
    }

    #[test]
    fn test_double_mut_borrow() {
        let errors = check(
            r#"
            fn test() {
                let x = 10;
                let r = &mut x;
                let s = &mut x;
            }
            "#,
        );
        assert!(has_error(&errors, |e| matches!(
            e,
            BorrowError::ConflictingBorrow { .. }
        )));
    }

    #[test]
    fn test_reborrow_after_scope() {
        let errors = check(
            r#"
            fn test() {
                let x = 10;
                {
                    let r = &x;
                    let s = &x;
                }
                let t = &mut x;
            }
            "#,
        );
        // Immutable borrows expire, mutable borrow is ok
        assert!(errors.is_empty());
    }

    #[test]
    fn test_static_lifetime_ok() {
        let errors = check(
            r#"
            fn test(x: &'static i32) -> &'static i32 {
                x
            }
            "#,
        );
        // 'static references never dangle
        assert!(errors.is_empty());
    }

    #[test]
    fn test_string_literal_no_dangling() {
        let errors = check(
            r#"
            fn test() -> &str {
                "hello"
            }
            "#,
        );
        // String literals are &'static str, never dangle
        assert!(errors.is_empty());
    }

    #[test]
    fn test_string_literal_in_block() {
        let errors = check(
            r#"
            fn test() -> &str {
                let s = {
                    "world"
                };
                s
            }
            "#,
        );
        // String literals propagate their 'static origin through blocks
        assert!(errors.is_empty());
    }

    #[test]
    fn test_string_literal_assigned_to_variable() {
        let errors = check(
            r#"
            fn test() -> &str {
                let s = "hello";
                s
            }
            "#,
        );
        // String literals propagate 'static through variable bindings
        assert!(errors.is_empty());
    }

    #[test]
    fn test_bool_literals() {
        let errors = check(
            r#"
            fn test() {
                let t = true;
                let f = false;
            }
            "#,
        );
        // Bool literals are Copy, no borrow issues
        assert!(errors.is_empty());
    }
}
