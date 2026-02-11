use std::collections::HashMap;

pub struct ScopedEnvironment<T> {
    scopes: Vec<HashMap<String, T>>,
}

impl<T> Default for ScopedEnvironment<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ScopedEnvironment<T> {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn leave_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn insert(&mut self, name: impl Into<String>, value: T) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.into(), value);
        }
    }

    pub fn get(&self, name: &str) -> Option<&T> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value);
            }
        }
        None
    }

    pub fn depth(&self) -> usize {
        self.scopes.len() - 1
    }
}
