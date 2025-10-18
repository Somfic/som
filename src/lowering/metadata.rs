use std::collections::HashSet;

/// Metadata collected during the lowering pass
#[derive(Debug, Clone, Default)]
pub struct LoweringMetadata {
    /// Set of function names that should use tail-call optimization
    pub tail_recursive_functions: HashSet<String>,
}

impl LoweringMetadata {
    pub fn new() -> Self {
        Self {
            tail_recursive_functions: HashSet::new(),
        }
    }

    pub fn mark_tail_recursive(&mut self, name: String) {
        self.tail_recursive_functions.insert(name);
    }

    pub fn is_tail_recursive(&self, name: &str) -> bool {
        self.tail_recursive_functions.contains(name)
    }
}
