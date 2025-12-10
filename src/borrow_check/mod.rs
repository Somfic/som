mod error;
use std::collections::HashMap;

pub use error::*;

use crate::{Expr, ExprId, Stmt, StmtId, Type, TypedAst};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlaceId(u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LoanId(u32);

pub struct Place {
    name: String,
    scope_depth: u32,
}

#[derive(Clone)]
pub enum State {
    Owned,
    Moved { to: ExprId },
    Borrowed { loans: Vec<LoanId> },
    MutBorrowed { loan: LoanId },
}

pub enum ReferenceOrigin {
    Local { at: PlaceId, scope_depth: u32 },
    Parameter,
    Static,
}

pub struct Loan {
    at: PlaceId,
    mutable: bool,
    expr: ExprId,
    scope_depth: u32,
}

pub struct BorrowChecker<'a> {
    typed_ast: &'a TypedAst,                       // The input AST with types
    places: Vec<Place>,                            // All places
    name_to_place: HashMap<String, PlaceId>,       // Lookup by name
    place_states: HashMap<PlaceId, State>,         // Current state
    ref_origins: HashMap<ExprId, ReferenceOrigin>, // Where refs point
    loans: Vec<Loan>,                              // All loans created
    scope_depth: u32,                              // Current nesting level
    errors: Vec<BorrowError>,                      // Collected errors
    next_place: u32,                               // ID counter
    next_loan: u32,                                // ID counter
}

impl<'a> BorrowChecker<'a> {
    pub fn new(typed_ast: &'a TypedAst) -> Self {
        Self {
            typed_ast,
            places: vec![],
            name_to_place: HashMap::new(),
            place_states: HashMap::new(),
            ref_origins: HashMap::new(),
            loans: vec![],
            scope_depth: 0,
            errors: vec![],
            next_place: 0,
            next_loan: 0,
        }
    }

    /// checks if a variable can be read
    fn check_read(&mut self, name: impl Into<String>, expr_id: ExprId) {
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
                let loan = &self.loans[loan.0 as usize];
                BorrowError::UseWhileMutBorrowed {
                    name,
                    use_expr: expr_id,
                    borrow_expr: loan.expr,
                }
            }
        };

        self.errors.push(error);
    }

    fn check_move(&mut self, expr_id: ExprId) {
        // copy types are just copied, no need to check moves
        if let Some(ty) = self.typed_ast.types.get(&expr_id) {
            if self.is_copy(ty) {
                self.check_expr(expr_id);
                return;
            }
        }

        let expr = self.typed_ast.ast.get_expr(&expr_id);

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
                let loan = &self.loans[loans[0].0 as usize];
                self.errors.push(BorrowError::MoveWhileBorrowed {
                    name: name.value.to_string(),
                    move_expr: expr_id,
                    borrow_expr: loan.expr,
                });
            }
            State::MutBorrowed { loan } => {
                // report error
                let loan_data = &self.loans[loan.0 as usize];
                self.errors.push(BorrowError::MoveWhileBorrowed {
                    name: name.value.to_string(),
                    move_expr: expr_id,
                    borrow_expr: loan_data.expr,
                });
            }
        }
    }

    fn check_borrow(&mut self, mutable: bool, inner_expr: ExprId, borrow_expr: ExprId) {
        let inner = self.typed_ast.ast.get_expr(&inner_expr);

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
        let place = &self.places[place_id.0 as usize];
        let origin = if place.scope_depth == 0 {
            ReferenceOrigin::Parameter
        } else {
            ReferenceOrigin::Local {
                at: place_id,
                scope_depth: place.scope_depth,
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
                let existing = &self.loans[loan.0 as usize];
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
                    let existing = &self.loans[loans[0].0 as usize];
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

    fn check_expr(&mut self, expr_id: ExprId) {
        let expr = self.typed_ast.ast.get_expr(&expr_id).clone();

        match expr {
            Expr::Hole | Expr::I32(_) => {
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
                    // TODO: check_return for dangling refs
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
        }
    }

    fn check_stmt(&mut self, stmt_id: StmtId) {
        let stmt = self.typed_ast.ast.get_stmt(&stmt_id).clone();

        match stmt {
            Stmt::Let { name, value, .. } => {
                // move the RHS value
                self.check_move(*value);

                // create a place for the new binding
                self.fresh_place(&*name.value);
            }
        }
    }

    fn fresh_place(&mut self, name: impl Into<String>) -> PlaceId {
        let place_id = PlaceId(self.next_place);
        self.next_place += 1;

        let place = Place {
            name: name.into(),
            scope_depth: self.scope_depth,
        };

        self.name_to_place.insert(place.name.clone(), place_id);

        self.places.push(place);
        self.place_states.insert(place_id, State::Owned);

        place_id
    }

    fn fresh_loan(&mut self, at: PlaceId, mutable: bool, expr: ExprId) -> LoanId {
        let loan_id = LoanId(self.next_loan);
        self.next_loan += 1;

        let loan = Loan {
            at,
            expr,
            mutable,
            scope_depth: self.scope_depth,
        };
        self.loans.push(loan);

        loan_id
    }

    fn push_scope(&mut self) {
        self.scope_depth += 1;
    }

    fn pop_scope(&mut self) {
        // find loans that expire
        let loans = self
            .loans
            .iter()
            .enumerate()
            .filter(|(_, loan)| loan.scope_depth == self.scope_depth)
            .map(|(idx, _)| LoanId(idx as u32));

        // process expiring loans
        for loan_id in loans {
            let place_id = self.loans[loan_id.0 as usize].at;

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

        // remove locals
        let names_to_remove = self
            .places
            .iter()
            .filter(|p| p.scope_depth == self.scope_depth)
            .map(|p| p.name.clone());

        // remove names
        for name in names_to_remove {
            self.name_to_place.remove(&name);
        }

        self.scope_depth -= 1;
    }

    fn is_copy(&self, ty: &Type) -> bool {
        matches!(
            ty,
            Type::Unit | Type::Bool | Type::I32 | Type::Reference { .. }
        )
    }
}
