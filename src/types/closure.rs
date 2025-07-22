use crate::prelude::*;
use std::collections::HashMap;

/// Represents a captured variable in a closure
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapturedVariable {
    /// The name of the captured variable
    pub name: String,
    /// The type of the captured variable
    pub type_: TypeValue,
    /// The scope level where this variable was originally declared
    /// (0 = immediate parent, 1 = grandparent, etc.)
    pub scope_level: usize,
    /// Whether this variable is captured by value or by reference
    pub capture_mode: CaptureMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CaptureMode {
    /// Capture by value (copy the variable's value into the closure)
    ByValue,
    /// Capture by reference (share the variable with the outer scope)
    ByReference,
}

/// Represents a closure type - a function with captured variables
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureType {
    /// The function signature (parameters and return type)
    pub function_type: FunctionType,
    /// Variables captured from the enclosing scope
    pub captured_variables: HashMap<String, CapturedVariable>,
    /// The span where this closure type is defined
    pub span: Span,
}

impl ClosureType {
    pub fn new(
        function_type: FunctionType,
        captured_variables: HashMap<String, CapturedVariable>,
        span: Span,
    ) -> Self {
        Self {
            function_type,
            captured_variables,
            span,
        }
    }

    /// Check if this closure captures any variables
    pub fn is_pure_function(&self) -> bool {
        self.captured_variables.is_empty()
    }

    /// Get the underlying function type
    pub fn as_function_type(&self) -> &FunctionType {
        &self.function_type
    }
}

/// Analysis result for a function body to determine what variables it captures
#[derive(Debug, Clone)]
pub struct CaptureAnalysis {
    /// Variables that are captured from outer scopes
    pub captured_variables: HashMap<String, CapturedVariable>,
    /// Variables that are declared locally in this function
    pub local_variables: HashMap<String, TypeValue>,
    /// Variables that are referenced but not found in any scope (errors)
    pub unresolved_variables: Vec<String>,
}

impl CaptureAnalysis {
    pub fn new() -> Self {
        Self {
            captured_variables: HashMap::new(),
            local_variables: HashMap::new(),
            unresolved_variables: Vec::new(),
        }
    }

    pub fn add_captured_variable(&mut self, name: String, captured_var: CapturedVariable) {
        self.captured_variables.insert(name, captured_var);
    }

    pub fn add_local_variable(&mut self, name: String, type_: TypeValue) {
        self.local_variables.insert(name, type_);
    }

    pub fn add_unresolved_variable(&mut self, name: String) {
        self.unresolved_variables.push(name);
    }

    pub fn is_variable_local(&self, name: &str) -> bool {
        self.local_variables.contains_key(name)
    }

    pub fn is_variable_captured(&self, name: &str) -> bool {
        self.captured_variables.contains_key(name)
    }
}
