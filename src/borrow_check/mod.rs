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
            && ty.is_copy()
        {
            self.check_expr(expr_id);
            return;
        }

        let expr = self.typed_ast.ast.exprs.get(&expr_id);

        // for non-variables, just check the expression
        let Expr::Var(path) = expr else {
            self.check_expr(expr_id);
            return;
        };

        // check if readable
        self.check_read(path.name().value.clone(), expr_id);

        // find the place and state
        let Some(&place_id) = self.name_to_place.get(&*path.name().value) else {
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
                    name: path.name().value.to_string(),
                    move_expr: expr_id,
                    borrow_expr: loan.expr,
                });
            }
            State::MutBorrowed { loan } => {
                // report error
                let loan_data = self.loans.get(&loan);
                self.errors.push(BorrowError::MoveWhileBorrowed {
                    name: path.name().value.to_string(),
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
        let Some(&place_id) = self.name_to_place.get(&*name.name().to_string()) else {
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
                    name: name.name().to_string(),
                    use_expr: borrow_expr,
                    moved_at: to,
                });
            }
            State::MutBorrowed { loan } => {
                // already borrowed
                let existing = self.loans.get(&loan);
                self.errors.push(BorrowError::ConflictingBorrow {
                    name: name.name().to_string(),
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
                        name: name.name().to_string(),
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
            Expr::Hole | Expr::I32(_) | Expr::F32(_) | Expr::Bool(_) | Expr::String(_) => {
                // Literals - nothing to check
            }
            Expr::Var(name) => {
                self.check_read(&*name.name().to_string(), expr_id);
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
                            let place_id = self.name_to_place.get(&*name.name().to_string())?;
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
            Expr::Not { expr } => {
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

                let truthy_origin = self.ref_origins.get(truthy).cloned();
                let falsy_origin = self.ref_origins.get(falsy).cloned();

                let origin = match (&truthy_origin, &falsy_origin) {
                    (Some(t), Some(f)) => {
                        // Pick the one with higher scope_depth (more local = more dangerous)
                        let t_depth = match t {
                            ReferenceOrigin::Local { scope_depth, .. } => *scope_depth,
                            ReferenceOrigin::Parameter => 0,
                            ReferenceOrigin::Static => 0,
                        };
                        let f_depth = match f {
                            ReferenceOrigin::Local { scope_depth, .. } => *scope_depth,
                            ReferenceOrigin::Parameter => 0,
                            ReferenceOrigin::Static => 0,
                        };
                        if t_depth >= f_depth {
                            truthy_origin
                        } else {
                            falsy_origin
                        }
                    }
                    (Some(_), None) => truthy_origin,
                    (None, Some(_)) => falsy_origin,
                    (None, None) => None,
                };

                if let Some(origin) = origin {
                    self.ref_origins.insert(expr_id, origin);
                }
            }
            Expr::Constructor { fields, .. } => {
                // Each field value is moved into the struct
                for (_, value) in fields {
                    self.check_move(*value);
                }
            }
            Expr::FieldAccess { object, .. } => {
                // Check the object expression (field access doesn't move)
                self.check_expr(*object);
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
            Stmt::Loop { body } => {
                for stmt in body {
                    self.check_stmt(*stmt);
                }
            }
            Stmt::While { condition, body } => {
                self.check_expr(*condition);
                for stmt in body {
                    self.check_stmt(*stmt);
                }
            }
            Stmt::Condition {
                condition,
                then_body,
                else_body,
            } => {
                self.check_expr(*condition);
                for stmt in then_body {
                    self.check_stmt(*stmt);
                }
                if let Some(else_stmts) = else_body {
                    for stmt in else_stmts {
                        self.check_stmt(*stmt);
                    }
                }
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
}
